/*!
 * Test application for ITCH protocol parser and order book reconstruction
 * 
 * This application directly tests the ITCH implementation without 
 * involving the full TEE client architecture.
 */

use aristo_client::itch::{ITCHParser, OrderBookReconstructor};
use std::path::PathBuf;
use std::fs::File;
use std::io::{Read, BufReader};
use std::env;

fn main() {
    println!("ITCH Parser Test Application");
    println!("----------------------------");
    
    // Process command line arguments
    let args: Vec<String> = env::args().collect();
    let sample_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("tests/data/sample.itch")
    };
    
    let symbol = if args.len() > 2 {
        args[2].clone()
    } else {
        "AAPL".to_string()
    };
    
    println!("Using sample file: {:?}", sample_path);
    println!("Using symbol: {}", symbol);
    
    // Read the sample file
    let mut file = match File::open(&sample_path) {
        Ok(file) => file,
        Err(e) => {
            println!("Error opening file: {}", e);
            return;
        }
    };
    
    let mut data = Vec::new();
    if let Err(e) = file.read_to_end(&mut data) {
        println!("Error reading file: {}", e);
        return;
    }
    
    println!("Read {} bytes from sample file", data.len());
    
    // Process messages
    let mut parser = ITCHParser::new();
    let mut book_reconstructor = OrderBookReconstructor::new();
    let mut message_count = 0;
    
    let mut pos = 0;
    while pos + 2 <= data.len() {
        // Read message size (2 bytes, big endian)
        let size = ((data[pos] as u16) << 8) | (data[pos + 1] as u16);
        pos += 2;
        
        if pos + size as usize > data.len() {
            println!("Incomplete message at position {}", pos);
            break;
        }
        
        // Extract message data
        let message_data = &data[pos..pos + size as usize];
        pos += size as usize;
        
        // Parse the message
        match parser.parse_message(message_data) {
            Ok(message) => {
                println!("\nMessage {}: Type {:?}", message_count, message.message_type);
                
                if let Some(stock) = &message.stock {
                    println!("  Stock: {}", stock);
                }
                
                // Update the order book
                if let Err(e) = book_reconstructor.process_message(&message) {
                    println!("  Error processing message: {:?}", e);
                }
                
                message_count += 1;
            },
            Err(e) => {
                println!("Error parsing message at position {}: {:?}", pos, e);
                break;
            }
        }
    }
    
    println!("\nProcessed {} messages", message_count);
    
    // Print book statistics
    let stats = book_reconstructor.get_statistics();
    println!("\nOrder Book Statistics:");
    for (key, value) in &stats {
        println!("  {}: {}", key, value);
    }
    
    // Display order books for found symbols
    let symbols = book_reconstructor.get_symbols();
    println!("\nFound {} symbols with order book data", symbols.len());
    
    for symbol_name in &symbols {
        if let Some(book) = book_reconstructor.get_order_book(symbol_name) {
            println!("\nOrder Book for {}:", symbol_name);
            println!("  Bids: {}", book.bids.len());
            for (i, bid) in book.bids.iter().enumerate() {
                println!("    ${:.4} - {} shares ({} orders)", 
                    bid.price, bid.size, bid.order_count);
                if i >= 4 { 
                    println!("    ... {} more bid levels", book.bids.len() - 5);
                    break;
                }
            }
            
            println!("  Asks: {}", book.asks.len());
            for (i, ask) in book.asks.iter().enumerate() {
                println!("    ${:.4} - {} shares ({} orders)", 
                    ask.price, ask.size, ask.order_count);
                if i >= 4 { 
                    println!("    ... {} more ask levels", book.asks.len() - 5);
                    break;
                }
            }
        }
    }
    
    // If the specified symbol exists, show some detailed output
    if symbols.contains(&symbol) {
        if let Some(book) = book_reconstructor.get_order_book(&symbol) {
            println!("\nDetailed Order Book for {}:", symbol);
            println!("  Timestamp: {}", book.timestamp);
            
            // Display top 5 bids and asks in a table format
            println!("\n  Market Depth:");
            println!("  {:-^60}", "");
            println!("  {:^30}|{:^30}", "Bids", "Asks");
            println!("  {:-^60}", "");
            println!("  {:^10} {:^10} {:^8} | {:^10} {:^10} {:^8}", 
                "Price", "Size", "Orders", "Price", "Size", "Orders");
            println!("  {:-^60}", "");
            
            let bid_count = book.bids.len();
            let ask_count = book.asks.len();
            
            for i in 0..5 {
                let bid_str = if i < bid_count {
                    format!("${:.4} {:10} {:8}", 
                        book.bids[i].price, book.bids[i].size, book.bids[i].order_count)
                } else {
                    "                      ".to_string()
                };
                
                let ask_str = if i < ask_count {
                    format!("${:.4} {:10} {:8}", 
                        book.asks[i].price, book.asks[i].size, book.asks[i].order_count)
                } else {
                    "                      ".to_string()
                };
                
                println!("  {} | {}", bid_str, ask_str);
            }
            
            // Calculate spread
            if !book.bids.is_empty() && !book.asks.is_empty() {
                let best_bid = book.bids[0].price;
                let best_ask = book.asks[0].price;
                let spread = best_ask - best_bid;
                let spread_pct = (spread / best_bid) * 100.0;
                
                println!("\n  Spread: ${:.4} ({:.4}%)", spread, spread_pct);
            }
        } else {
            println!("\nSymbol {} not found in processed data", symbol);
        }
    }
    
    println!("\nTest completed successfully!");
}
