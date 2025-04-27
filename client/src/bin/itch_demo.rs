/*!
 * ITCH Protocol Demo
 * 
 * This binary demonstrates the core functionality of the NASDAQ ITCH
 * protocol parser and integration with the TEE parameter formats.
 */

use aristo_client::itch::parser::ITCHParser;
use aristo_client::itch::types::OrderBook;
use aristo_client::itch::types::*;
use aristo_client::protocol::types::ParameterFormat;
use std::collections::HashMap;

fn main() {
    println!("NASDAQ ITCH Protocol Integration Demo");
    println!("======================================");
    
    // Initialize the ITCH parser
    let parser = ITCHParser::new();
    println!("Initialized ITCH parser");
    
    // Create sample messages
    let messages = create_sample_messages();
    println!("Created {} sample messages", messages.len());
    
    // Process each message
    let mut order_books: HashMap<String, OrderBook> = HashMap::new();
    
    for (i, message) in messages.iter().enumerate() {
        println!("\nProcessing message #{}: {:?}", i+1, message.message_type);
        
        // Update order book
        let book = order_books
            .entry(message.stock.clone().unwrap_or_else(|| "UNKNOWN".to_string()))
            .or_insert_with(OrderBook::new);
            
        book.process_message(message).unwrap();
        
        // Print order book state
        println!("Order book for {}: {} bids, {} asks", 
            message.stock.as_ref().unwrap_or(&"UNKNOWN".to_string()), 
            book.bids.len(), 
            book.asks.len());
            
        // Demonstrate parameter format handling for TEE contracts
        let length_prefixed = parser.prepare_for_contract(message, ParameterFormat::LengthPrefixed)
            .expect("Failed to prepare length-prefixed parameters");
            
        let direct = parser.prepare_for_contract(message, ParameterFormat::Direct)
            .expect("Failed to prepare direct parameters");
            
        println!("Parameter formats:");
        println!("  Length-prefixed: {} bytes", length_prefixed.len());
        println!("  Direct: {} bytes", direct.len());
        
        // Verify length-prefixed format has the correct structure
        if length_prefixed.len() >= 4 {
            let mut len_bytes = [0u8; 4];
            len_bytes.copy_from_slice(&length_prefixed[0..4]);
            let data_len = u32::from_le_bytes(len_bytes);
            println!("  Length prefix value: {} bytes", data_len);
            
            if data_len as usize == length_prefixed.len() - 4 {
                println!("  ✅ Length prefix is valid");
            } else {
                println!("  ❌ Length prefix is invalid");
            }
        }
    }
    
    // Show final order book state
    for (symbol, book) in &order_books {
        println!("\nFinal order book for {}:", symbol);
        println!("  Top of book:");
        
        if !book.bids.is_empty() {
            let best_bid = &book.bids[0];
            println!("    Best bid: {} @ ${:.2}", best_bid.size, best_bid.price);
        } else {
            println!("    No bids");
        }
        
        if !book.asks.is_empty() {
            let best_ask = &book.asks[0];
            println!("    Best ask: {} @ ${:.2}", best_ask.size, best_ask.price);
        } else {
            println!("    No asks");
        }
        
        // Calculate spread if possible
        if !book.bids.is_empty() && !book.asks.is_empty() {
            let best_bid = &book.bids[0];
            let best_ask = &book.asks[0];
            let spread = best_ask.price - best_bid.price;
            let spread_pct = (spread / best_bid.price) * 100.0;
            
            println!("    Spread: ${:.2} ({:.2}%)", spread, spread_pct);
        }
    }
    
    println!("\nITCH integration demo completed successfully!");
}

// Create sample ITCH messages
fn create_sample_messages() -> Vec<ITCHMessage> {
    let mut messages = Vec::new();
    
    // Add Order (Buy)
    messages.push(ITCHMessage {
        message_type: MessageType::AddOrder,
        stock: Some("AAPL".to_string()),
        timestamp: 34200000000000, // 9:30:00.000000000
        payload: MessagePayload::AddOrder(AddOrderMessage {
            order_reference_number: 12345678,
            buy_sell_indicator: BuySellIndicator::Buy,
            shares: 100,
            stock: "AAPL".to_string(), // AddOrderMessage uses String, not Option<String>
            price: (170.45 * 10000.0) as u64, // Convert from dollars to 10^-4 dollars
        }),
    });
    
    // Add Order (Sell)
    messages.push(ITCHMessage {
        message_type: MessageType::AddOrder,
        stock: Some("AAPL".to_string()),
        timestamp: 34200100000000, // 9:30:00.100000000
        payload: MessagePayload::AddOrder(AddOrderMessage {
            order_reference_number: 12345679,
            buy_sell_indicator: BuySellIndicator::Sell,
            shares: 50,
            stock: "AAPL".to_string(), // AddOrderMessage uses String, not Option<String>
            price: (170.55 * 10000.0) as u64, // Convert from dollars to 10^-4 dollars
        }),
    });
    
    // Add another Buy Order
    messages.push(ITCHMessage {
        message_type: MessageType::AddOrder,
        stock: Some("AAPL".to_string()),
        timestamp: 34200200000000, // 9:30:00.200000000
        payload: MessagePayload::AddOrder(AddOrderMessage {
            order_reference_number: 12345680,
            buy_sell_indicator: BuySellIndicator::Buy,
            shares: 200,
            stock: "AAPL".to_string(), // AddOrderMessage uses String, not Option<String>
            price: (170.40 * 10000.0) as u64, // Convert from dollars to 10^-4 dollars
        }),
    });
    
    // Order Executed
    messages.push(ITCHMessage {
        message_type: MessageType::OrderExecuted,
        stock: Some("AAPL".to_string()),
        timestamp: 34200300000000, // 9:30:00.300000000
        payload: MessagePayload::OrderExecuted(OrderExecutedMessage {
            order_reference_number: 12345679, // Execute part of the sell order
            executed_shares: 30,
            match_number: 987654321,
        }),
    });
    
    // Order Delete
    messages.push(ITCHMessage {
        message_type: MessageType::OrderDelete,
        stock: Some("AAPL".to_string()),
        timestamp: 34200400000000, // 9:30:00.400000000
        payload: MessagePayload::OrderDelete(OrderDeleteMessage {
            order_reference_number: 12345680, // Delete the second buy order
        }),
    });
    
    messages
}
