/*!
 * NASDAQ ITCH Parser Example
 * 
 * This example demonstrates how to use the ITCH client to process market data files
 * and reconstruct order books for TEE contract execution.
 */

use aristo_client::itch::{ITCHClient, OrderBook};
use aristo_client::protocol::ParameterFormat;
use std::path::PathBuf;
use std::env;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <itch_file_path> [symbol]", args[0]);
        println!("Example: {} ./data/S030123-v50.txt AAPL", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let symbol = args.get(2).map(|s| s.as_str()).unwrap_or("AAPL");

    println!("NASDAQ ITCH Parser Example");
    println!("---------------------------");
    println!("File: {}", file_path);
    println!("Symbol: {}", symbol);
    println!();

    // Create a new ITCH client with debug enabled
    let client = ITCHClient::new().with_debug(true);
    
    // Process the ITCH file
    println!("Processing ITCH file...");
    client.process_file(file_path)?;
    
    // Display statistics
    println!("\nParser Statistics:");
    let parser_stats = client.get_parser_statistics().await?;
    for (key, value) in parser_stats.iter() {
        println!("  {}: {}", key, value);
    }
    
    println!("\nOrder Book Statistics:");
    let book_stats = client.get_book_statistics().await?;
    for (key, value) in book_stats.iter() {
        println!("  {}: {}", key, value);
    }
    
    // Get available symbols
    println!("\nAvailable Symbols:");
    let symbols = client.get_symbols().await?;
    for (i, s) in symbols.iter().enumerate().take(20) {
        print!("{} ", s);
        if (i + 1) % 5 == 0 {
            println!();
        }
    }
    if symbols.len() > 20 {
        println!("... and {} more", symbols.len() - 20);
    } else {
        println!();
    }
    
    // Retrieve the order book for the specified symbol
    if symbols.contains(&symbol.to_string()) {
        println!("\nOrder Book for {}:", symbol);
        let order_book = client.get_order_book(symbol).await?;
        if let Some(book) = order_book {
            display_order_book(&book);
            
            // Demonstrate WebAssembly parameter format transformation
            let length_prefixed = client.transform_order_book(symbol, ParameterFormat::LengthPrefixed).await?;
            let direct = client.transform_order_book(symbol, ParameterFormat::Direct).await?;
            
            println!("\nParameter Format Sizes:");
            println!("  Length-Prefixed: {} bytes", length_prefixed.len());
            println!("  Direct: {} bytes", direct.len());
            
            // Verify that the length prefix matches the actual data length
            if length_prefixed.len() >= 4 {
                let length_bytes = [
                    length_prefixed[0], 
                    length_prefixed[1], 
                    length_prefixed[2], 
                    length_prefixed[3]
                ];
                let length = u32::from_le_bytes(length_bytes) as usize;
                println!("  Length Prefix Value: {} bytes", length);
                assert_eq!(length_prefixed.len() - 4, length, "Length prefix mismatch");
                println!("  Length Verification: ✓");
            }
        } else {
            println!("No order book data available for {}", symbol);
        }
    } else {
        println!("\nSymbol {} not found in the processed data", symbol);
    }
    
    Ok(())
}

fn display_order_book(book: &OrderBook) {
    println!("  Symbol: {}", book.symbol);
    println!("  Timestamp: {}", book.timestamp);
    
    // Display top 5 bids
    println!("\n  Top Bids:");
    println!("  {:^12} | {:^10} | {:^5}", "Price", "Size", "Orders");
    println!("  {}", "-".repeat(31));
    for (i, entry) in book.bids.iter().enumerate().take(5) {
        println!("  ${:10.4} | {:10} | {:5}", 
            entry.price, 
            entry.size, 
            entry.order_count);
    }
    
    // Display top 5 asks
    println!("\n  Top Asks:");
    println!("  {:^12} | {:^10} | {:^5}", "Price", "Size", "Orders");
    println!("  {}", "-".repeat(31));
    for (i, entry) in book.asks.iter().enumerate().take(5) {
        println!("  ${:10.4} | {:10} | {:5}", 
            entry.price, 
            entry.size, 
            entry.order_count);
    }
    
    // Additional statistics
    println!("\n  Book Depth: {} bids, {} asks", book.bids.len(), book.asks.len());
    
    // Calculate spread if possible
    if !book.bids.is_empty() && !book.asks.is_empty() {
        let best_bid = book.bids[0].price;
        let best_ask = book.asks[0].price;
        let spread = best_ask - best_bid;
        let spread_percent = (spread / best_bid) * 100.0;
        
        println!("  Spread: ${:.4} ({:.4}%)", spread, spread_percent);
    }
}
