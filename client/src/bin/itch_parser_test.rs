/*!
 * ITCH Parser Test
 * 
 * Simple standalone test for the ITCH parser and order book reconstruction.
 */

use aristo_client::itch::{ITCHParser, OrderBookReconstructor};
use aristo_client::protocol::ParameterFormat;

fn main() {
    println!("ITCH Parser Test");
    println!("--------------");
    
    // Test the parser
    test_itch_parser();
    
    // Test order book reconstruction
    test_order_book_reconstruction();
    
    println!("\nAll tests completed successfully!");
}

/// Test the ITCH message parser
fn test_itch_parser() {
    println!("\nTesting ITCH parser...");
    
    let mut parser = ITCHParser::new();
    
    // Test parsing a simple System Event message
    let message_type = b'S'; // System Event
    let timestamp = 1234567890u64.to_be_bytes();
    let event_code = b'O'; // Start of Messages
    
    let mut message_data = vec![message_type]; // message_type is already a u8, not an array
    message_data.extend_from_slice(&timestamp);
    message_data.extend_from_slice(&[event_code]); // Wrap single byte in array
    
    let result = parser.parse_message(&message_data);
    assert!(result.is_ok(), "Failed to parse system event");
    
    let message = result.unwrap();
    println!("Successfully parsed message type: {:?}", message.message_type);
    println!("Timestamp: {}", message.timestamp);
    
    // Test parsing an Add Order message
    let message_type = b'A'; // Add Order
    let timestamp = 9876543210u64.to_be_bytes();
    let order_ref = 12345u64.to_be_bytes();
    let buy_sell = b'B'; // Buy
    let shares = 100u32.to_be_bytes();
    let stock = b"AAPL    "; // 8 characters, space-padded
    let price = 150000u32.to_be_bytes(); // $15.00 * 10000
    
    let mut message_data = vec![message_type]; // message_type is already a u8, not an array
    message_data.extend_from_slice(&timestamp);
    message_data.extend_from_slice(&order_ref);
    message_data.extend_from_slice(&[buy_sell]); // Wrap single byte in array
    message_data.extend_from_slice(&shares);
    message_data.extend_from_slice(stock);
    message_data.extend_from_slice(&price);
    
    let result = parser.parse_message(&message_data);
    assert!(result.is_ok(), "Failed to parse add order");
    
    let message = result.unwrap();
    println!("Successfully parsed message type: {:?}", message.message_type);
    if let Some(stock) = &message.stock {
        println!("Stock: {}", stock);
    }
    
    println!("Parser tests passed!");
}

/// Test order book reconstruction
fn test_order_book_reconstruction() {
    println!("\nTesting order book reconstruction...");
    
    let mut parser = ITCHParser::new();
    let mut book = OrderBookReconstructor::new();
    
    // Add some orders
    let orders = [
        // Buy orders for AAPL
        create_add_order(1001, 'B', 100, "AAPL", 150000), // $15.00
        create_add_order(1002, 'B', 200, "AAPL", 149000), // $14.90
        create_add_order(1003, 'B', 300, "AAPL", 151000), // $15.10 (best bid)
        
        // Sell orders for AAPL
        create_add_order(2001, 'S', 150, "AAPL", 152000), // $15.20 (best ask)
        create_add_order(2002, 'S', 250, "AAPL", 153000), // $15.30
        create_add_order(2003, 'S', 350, "AAPL", 152500), // $15.25
    ];
    
    // Process all the orders
    for msg_data in orders.iter() {
        let message = parser.parse_message(msg_data).unwrap();
        book.process_message(&message).unwrap();
    }
    
    // Check the order book for AAPL
    let aapl_book = book.get_order_book("AAPL").expect("Should have AAPL order book");
    
    println!("Order book for AAPL:");
    println!("  {} bids, {} asks", aapl_book.bids.len(), aapl_book.asks.len());
    
    // Verify bids (should be ordered highest to lowest)
    assert_eq!(aapl_book.bids.len(), 3, "Should have 3 bid levels");
    assert_eq!(aapl_book.bids[0].price, 15.10, "Best bid should be $15.10");
    assert_eq!(aapl_book.bids[0].size, 300, "Best bid size should be 300");
    
    // Verify asks (should be ordered lowest to highest)
    assert_eq!(aapl_book.asks.len(), 3, "Should have 3 ask levels");
    assert_eq!(aapl_book.asks[0].price, 15.20, "Best ask should be $15.20");
    assert_eq!(aapl_book.asks[0].size, 150, "Best ask size should be 150");
    
    // Print the top of book
    println!("Top of book:");
    println!("  Best bid: ${:.2} ({} shares)", 
        aapl_book.bids[0].price, aapl_book.bids[0].size);
    println!("  Best ask: ${:.2} ({} shares)",
        aapl_book.asks[0].price, aapl_book.asks[0].size);
    
    // Calculate and show spread
    let spread = aapl_book.asks[0].price - aapl_book.bids[0].price;
    let spread_percent = (spread / aapl_book.bids[0].price) * 100.0;
    println!("  Spread: ${:.2} ({:.2}%)", spread, spread_percent);
    
    // Test WebAssembly parameter format transformation
    let parameter_data = serde_json::to_vec(&aapl_book).unwrap();
    
    // Create length-prefixed format
    let mut length_prefixed = Vec::new();
    length_prefixed.extend_from_slice(&(parameter_data.len() as u32).to_le_bytes());
    length_prefixed.extend_from_slice(&parameter_data);
    
    println!("\nParameter formats:");
    println!("  Length-prefixed size: {} bytes", length_prefixed.len());
    println!("  Direct size: {} bytes", parameter_data.len());
    
    // Verify length prefix
    let length_bytes = [
        length_prefixed[0], 
        length_prefixed[1], 
        length_prefixed[2], 
        length_prefixed[3]
    ];
    let length = u32::from_le_bytes(length_bytes) as usize;
    assert_eq!(length, parameter_data.len(), "Length prefix should match data length");
    assert_eq!(length_prefixed.len(), parameter_data.len() + 4, "Length-prefixed format should be 4 bytes longer");
    
    println!("Order book reconstruction tests passed!");
}

/// Helper function to create an Add Order message
fn create_add_order(order_ref: u64, side: char, shares: u32, stock: &str, price: u32) -> Vec<u8> {
    let message_type = b'A'; // Add Order
    let timestamp = 9876543210u64.to_be_bytes();
    let order_ref = order_ref.to_be_bytes();
    let buy_sell = if side == 'B' { b'B' } else { b'S' };
    let shares = shares.to_be_bytes();
    
    // Create space-padded stock symbol (8 chars)
    let mut stock_bytes = [b' '; 8];
    for (i, byte) in stock.as_bytes().iter().enumerate().take(8) {
        stock_bytes[i] = *byte;
    }
    
    let price = price.to_be_bytes();
    
    let mut message_data = vec![message_type]; // message_type is already a u8, not an array
    message_data.extend_from_slice(&timestamp);
    message_data.extend_from_slice(&order_ref);
    message_data.extend_from_slice(&[buy_sell]); // Wrap single byte in array
    message_data.extend_from_slice(&shares);
    message_data.extend_from_slice(&stock_bytes);
    message_data.extend_from_slice(&price);
    
    message_data
}
