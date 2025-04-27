use aristo_client::itch::parser::ITCHParser;
use aristo_client::itch::types::OrderBook;
use aristo_client::itch::types::*;
use aristo_client::protocol::types::ParameterFormat;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, BufReader};
use std::path::PathBuf;

// Create sample ITCH messages for testing
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
            price: (170.45 * 10000.0) as u64, // Convert to NASDAQ price format (10^-4 dollars)
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
            price: (170.55 * 10000.0) as u64, // Convert to NASDAQ price format (10^-4 dollars)
        }),
    });
    
    // Order Executed (partial fill of sell order)
    messages.push(ITCHMessage {
        message_type: MessageType::OrderExecuted,
        stock: Some("AAPL".to_string()),
        timestamp: 34200300000000, // 9:30:00.300000000
        payload: MessagePayload::OrderExecuted(OrderExecutedMessage {
            order_reference_number: 12345679,
            executed_shares: 30,
            match_number: 987654321,
        }),
    });
    
    messages
}

#[test]
fn test_itch_parser() {
    // Simple test to verify ITCH message parsing and parameter format handling
    println!("Testing ITCH parser and parameter formats");
    // Create sample ITCH data
    println!("Creating sample ITCH messages");
    let sample_messages = create_sample_messages();
    println!("Created {} sample messages", sample_messages.len());
    
    // If a sample file exists, we could use it, but for now let's use synthetic data
    // which is more predictable for testing
    let parser = ITCHParser::new();
    let mut order_books: HashMap<String, OrderBook> = HashMap::new();
    
    // Process each message and test parameter format handling
    for (i, message) in sample_messages.iter().enumerate() {
        println!("\nProcessing message #{}: {:?}", i+1, message.message_type);
        
        // Update order book
        let book = order_books
            .entry(message.stock.clone().expect("Stock symbol must be present"))
            .or_insert_with(OrderBook::new);
            
        book.process_message(message).expect("Failed to process message");
        
        // Print order book state
        println!("Order book for {:?}: {} bids, {} asks", 
            message.stock, 
            book.bids.len(), 
            book.asks.len());
            
        // Test parameter format handling for TEE contracts
        println!("Testing parameter formats");
        
        // Length-prefixed format
        let length_prefixed = parser.transform_message(message, ParameterFormat::LengthPrefixed)
            .expect("Failed to prepare length-prefixed parameters");
            
        // Direct format
        let direct = parser.transform_message(message, ParameterFormat::Direct)
            .expect("Failed to prepare direct parameters");
            
        println!("  Length-prefixed: {} bytes", length_prefixed.len());
        println!("  Direct: {} bytes", direct.len());
        
        // Verify length-prefixed format has the correct structure
        assert!(length_prefixed.len() >= 4, "Length-prefixed parameters should be at least 4 bytes");
        
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&length_prefixed[0..4]);
        let data_len = u32::from_le_bytes(len_bytes);
        println!("  Length prefix value: {} bytes", data_len);
        
        assert_eq!(data_len as usize, length_prefixed.len() - 4, "Length prefix should match data length");
        
        // Direct format should be length_prefixed minus the 4-byte prefix
        assert_eq!(direct.len(), length_prefixed.len() - 4, "Direct format should be length_prefixed minus the 4-byte prefix");
    }
    
    // Verify order book state after processing all messages
    println!("\nVerifying final order book state");
    
    // We should have some order book data for AAPL
    let aapl_book = order_books.get("AAPL").expect("Should have order book for AAPL");
    println!("AAPL order book: {} bids, {} asks", aapl_book.bids.len(), aapl_book.asks.len());
    
    // Verify that our order book processing correctly tracked the messages we sent
    assert_eq!(aapl_book.bids.len(), 1, "Should have 1 bid after processing messages");
    assert_eq!(aapl_book.asks.len(), 1, "Should have 1 ask after processing messages");
}
