/*!
 * ITCH Parser Test
 * 
 * Simple standalone test for the ITCH parser components without
 * requiring the full client architecture.
 */

use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

// We'll directly use the parser and book modules to avoid dependency issues
mod itch {
    // Import necessary modules and types from our implementation
    pub use aristo_client::itch::parser::ITCHParser;
    pub use aristo_client::itch::types::OrderBook;
    pub use aristo_client::itch::types::*;
}

fn main() -> io::Result<()> {
    println!("ITCH Parser Test");
    println!("---------------");
    
    // Locate the sample file
    let sample_path = PathBuf::from("tests/data/sample.itch");
    println!("Loading sample data from: {:?}", sample_path);
    
    // Read the file
    let mut file = File::open(&sample_path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    
    println!("Read {} bytes from sample file", data.len());
    
    // Process messages manually
    let mut parser = itch::ITCHParser::new();
    let mut book_reconstructor = itch::OrderBook::new();
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
                
                match &message.payload {
                    itch::MessagePayload::SystemEvent(sys_event) => {
                        println!("  System Event: {:?}", sys_event.event_code);
                    },
                    itch::MessagePayload::AddOrder(add_order) => {
                        println!("\nStatistics:");
                        let stats = book_reconstructor.get_statistics();
                        println!("  Timestamp: {}", stats.get("timestamp").unwrap_or(&0));
                        println!("  Bid levels: {}", stats.get("bid_levels").unwrap_or(&0));
                        println!("  Ask levels: {}", stats.get("ask_levels").unwrap_or(&0));
                        println!("  Total bid size: {}", stats.get("total_bid_size").unwrap_or(&0));
                        println!("  Total ask size: {}", stats.get("total_ask_size").unwrap_or(&0));
                        println!("  Add Order:");
                        println!("    Order ID: {}", add_order.order_reference_number);
                        println!("    Side: {:?}", add_order.buy_sell_indicator);
                        println!("    Shares: {}", add_order.shares);
                        println!("    Stock: {}", add_order.stock);
                        println!("    Price: ${:.4}", itch::price_to_float(add_order.price));
                    },
                    itch::MessagePayload::OrderExecuted(exec) => {
                        println!("  Order Executed:");
                        println!("    Order ID: {}", exec.order_reference_number);
                        println!("    Executed Shares: {}", exec.executed_shares);
                        println!("    Match Number: {}", exec.match_number);
                    },
                    _ => {
                        println!("  Other message type");
                    }
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
    // In the current implementation, we're using a single OrderBook instance
    // instead of a multi-symbol reconstructor
    println!("\nOrder Book Statistics:");
    println!("  Bids: {}", book_reconstructor.bids.len());
    
    // Print order book details
    if !book_reconstructor.bids.is_empty() || !book_reconstructor.asks.is_empty() {
        println!("\nOrder Book Details:");
        println!("  Bids: {}", book_reconstructor.bids.len());
            for (i, bid) in book_reconstructor.bids.iter().enumerate() {
                println!("    ${:.4} - {} shares ({} orders)", 
                    bid.price, bid.size, bid.order_count);
                if i >= 4 { 
                    println!("    ... {} more bid levels", book_reconstructor.bids.len() - 5);
                    break;
                }
            }
            
            println!("  Asks: {}", book_reconstructor.asks.len());
            for (i, ask) in book_reconstructor.asks.iter().enumerate() {
                println!("    ${:.4} - {} shares ({} orders)", 
                    ask.price, ask.size, ask.order_count);
                if i >= 4 { 
                    println!("    ... {} more ask levels", book_reconstructor.asks.len() - 5);
                    break;
                }
            }
        }
    
    Ok(())
}
