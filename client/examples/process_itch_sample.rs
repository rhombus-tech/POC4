use aristo_client::itch::parser::ITCHParser;
use aristo_client::itch::types::OrderBook;
use aristo_client::protocol::types::ParameterFormat;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use std::time::Instant;

/// Process NASDAQ ITCH sample data file and prepare for WebAssembly contracts
/// This example demonstrates processing real ITCH data and preparing it for
/// both parameter format types (length-prefixed and direct)
fn main() -> io::Result<()> {
    // Configure file paths
    let sample_path = PathBuf::from("data/itch/samples/20190327.PSX_ITCH_50");
    
    if !sample_path.exists() {
        println!("Sample file not found. Please run the download_itch_samples.sh script first.");
        println!("Command: cd tools && bash download_itch_samples.sh");
        return Ok(());
    }
    
    println!("Processing NASDAQ ITCH sample file: {:?}", sample_path);
    let start = Instant::now();
    
    // Create parser and order books (one per stock symbol)
    let mut parser = ITCHParser::new();
    let mut order_books: HashMap<String, OrderBook> = HashMap::new();
    
    // Open and read the binary file
    let file = File::open(&sample_path)?;
    let mut reader = BufReader::new(file);
    
    // Read messages in chunks - ITCH messages are prefixed with 2-byte size
    let mut buffer = Vec::new();
    let mut message_buffer = Vec::new();
    let mut messages_processed = 0;
    let mut messages_with_stock = 0;
    let mut processed_stocks = HashMap::new();
    
    // For demo purposes, limit to first 100,000 messages
    const MAX_MESSAGES: usize = 100_000;
    
    // Most financial data files are structured with a 2-byte size prefix followed by the message
    reader.read_to_end(&mut buffer)?;
    
    println!("File loaded into memory. Size: {} bytes", buffer.len());
    println!("Processing messages...");
    
    let mut pos = 0;
    while pos + 2 <= buffer.len() && messages_processed < MAX_MESSAGES {
        // Read message size (2 bytes, big endian)
        let msg_size = ((buffer[pos] as u16) << 8) | (buffer[pos + 1] as u16);
        pos += 2;
        
        if pos + msg_size as usize > buffer.len() {
            println!("Reached end of file or corrupt message");
            break;
        }
        
        // Extract the message
        message_buffer.clear();
        message_buffer.extend_from_slice(&buffer[pos..pos + msg_size as usize]);
        pos += msg_size as usize;
        
        // Process the message
        match parser.parse_message(&message_buffer) {
            Ok(message) => {
                messages_processed += 1;
                
                // Track stocks we've seen
                if let Some(stock) = &message.stock {
                    messages_with_stock += 1;
                    
                    let count = processed_stocks.entry(stock.clone()).or_insert(0);
                    *count += 1;
                    
                    // Update order book for this stock
                    let book = order_books
                        .entry(stock.clone())
                        .or_insert_with(OrderBook::new);
                    
                    // Process message to update the book
                    if let Err(e) = book.process_message(&message) {
                        println!("Error processing message for stock {}: {:?}", stock, e);
                    }
                    
                    // Demonstrate both parameter formats for WebAssembly contracts
                    if messages_processed % 10000 == 0 {
                        // For demo purposes, show parameter conversion for both formats
                        match parser.prepare_for_contract(&message, ParameterFormat::LengthPrefixed) {
                            Ok(length_prefixed) => {
                                println!("Message {} for {} prepared in length-prefixed format: {} bytes", 
                                    messages_processed, stock, length_prefixed.len());
                                
                                // Verify the length prefix is correct
                                let prefix_len = u32::from_le_bytes([
                                    length_prefixed[0], length_prefixed[1], 
                                    length_prefixed[2], length_prefixed[3]
                                ]);
                                println!("  Length prefix: {} bytes", prefix_len);
                                println!("  Actual data length: {} bytes", length_prefixed.len() - 4);
                            },
                            Err(e) => println!("Error preparing length-prefixed format: {:?}", e),
                        }
                        
                        match parser.prepare_for_contract(&message, ParameterFormat::Direct) {
                            Ok(direct) => {
                                println!("Message {} for {} prepared in direct format: {} bytes", 
                                    messages_processed, stock, direct.len());
                            },
                            Err(e) => println!("Error preparing direct format: {:?}", e),
                        }
                    }
                }
                
                // Print progress
                if messages_processed % 10000 == 0 {
                    println!("Processed {} messages, {} with stock data, {} unique stocks", 
                        messages_processed, messages_with_stock, processed_stocks.len());
                }
            },
            Err(e) => {
                // Skip messages that fail to parse
                println!("Failed to parse message at position {}: {:?}", pos - msg_size as usize, e);
            }
        }
    }
    
    let duration = start.elapsed();
    println!("\nProcessing completed in {:.2?}", duration);
    println!("Total messages processed: {}", messages_processed);
    println!("Messages with stock data: {}", messages_with_stock);
    println!("Unique stocks encountered: {}", processed_stocks.len());
    
    // Display top stocks by message count
    let mut stock_counts: Vec<(String, usize)> = processed_stocks.iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    
    stock_counts.sort_by(|a, b| b.1.cmp(&a.1));
    
    println!("\nTop 10 stocks by message count:");
    for (idx, (stock, count)) in stock_counts.iter().take(10).enumerate() {
        println!("{}. {} - {} messages", idx + 1, stock, count);
        
        // Display order book summary for this stock
        if let Some(book) = order_books.get(stock) {
            let stats = book.get_statistics();
            println!("   Book stats - Bid levels: {}, Ask levels: {}", 
                stats.get("bid_levels").unwrap_or(&0),
                stats.get("ask_levels").unwrap_or(&0));
        }
    }
    
    // Demonstrate TEE execution preparation for WebAssembly contracts
    if let Some((top_stock, _)) = stock_counts.first() {
        if let Some(book) = order_books.get(top_stock) {
            println!("\nPreparing order book for {} for TEE execution:", top_stock);
            
            // In a real application, you would serialize the order book here
            // and prepare it for WebAssembly contract execution in the TEE
            // For this example, we'll just print some stats
            let stats = book.get_statistics();
            println!("Order book statistics for {}:", top_stock);
            for (key, value) in stats.iter() {
                println!("  {}: {}", key, value);
            }
            
            println!("\nThe order book can now be passed to WebAssembly contracts in the TEE");
            println!("using either the length-prefixed or direct parameter format.");
        }
    }
    
    Ok(())
}
