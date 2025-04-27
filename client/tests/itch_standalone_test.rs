/*!
 * ITCH Standalone Test
 * 
 * This test verifies the core functionality of the NASDAQ ITCH protocol parser
 * without requiring the full TEE environment.
 */

use aristo_client::itch::types::*;
use aristo_client::itch::parser::ITCHParser;
use aristo_client::protocol::types::ParameterFormat;

#[test]
fn test_itch_parser_functionality() {
    // Create a parser
    let mut parser = ITCHParser::new();
    
    // Create sample message data
    let msg_data = create_sample_add_order_message();
    
    // Parse the message
    let result = parser.parse_message(&msg_data);
    assert!(result.is_ok(), "Failed to parse message: {:?}", result.err());
    
    let message = result.unwrap();
    
    // Verify the parsed message
    assert_eq!(message.message_type, MessageType::AddOrder);
    assert_eq!(message.stock, Some("AAPL".to_string()));
    
    if let MessagePayload::AddOrder(add_order) = &message.payload {
        assert_eq!(add_order.order_reference_number, 12345678);
        assert_eq!(add_order.buy_sell_indicator, BuySellIndicator::Buy);
        assert_eq!(add_order.shares, 100);
        // Use price_to_float to convert from price integer (10^-4 dollars) to float for comparison
        assert_eq!(price_to_float(add_order.price), 170.45);
    } else {
        panic!("Expected AddOrder payload, got: {:?}", message.payload);
    }
    
    println!("Successfully parsed AddOrder message: {:?}", message);
}

#[test]
fn test_parameter_format_conversion() {
    // Create a parser
    let mut parser = ITCHParser::new();
    
    // Create sample message
    let msg_data = create_sample_add_order_message();
    let message = parser.parse_message(&msg_data).unwrap();
    
    // Test length-prefixed format conversion
    let params = parser.prepare_for_contract(&message, ParameterFormat::LengthPrefixed).unwrap();
    
    // Verify length prefix
    assert!(params.len() > 4, "Parameters should have at least 4 bytes for length prefix");
    let prefix_len = u32::from_le_bytes([params[0], params[1], params[2], params[3]]);
    assert_eq!(prefix_len as usize, params.len() - 4, "Length prefix should match data length");
    
    // Test direct format conversion
    let direct_params = parser.prepare_for_contract(&message, ParameterFormat::Direct).unwrap();
    
    // Direct format should not have length prefix
    assert_eq!(direct_params.len(), params.len() - 4, "Direct format should not have length prefix");
    
    println!("Successfully converted message to both parameter formats");
}

// Helper function to create a sample Add Order message in binary format
fn create_sample_add_order_message() -> Vec<u8> {
    let mut data = Vec::new();
    
    // Message Type (A = Add Order)
    data.push(b'A');
    
    // Timestamp (nanoseconds since midnight) - must come immediately after message type
    let timestamp: u64 = 34200000000000; // 9:30:00.000000000
    data.extend_from_slice(&timestamp.to_be_bytes());
    
    // Order Reference Number
    let order_ref: u64 = 12345678;
    data.extend_from_slice(&order_ref.to_be_bytes());
    
    // Buy/Sell Indicator (B = Buy)
    data.push(b'B');
    
    // Shares
    let shares: u32 = 100;
    data.extend_from_slice(&shares.to_be_bytes());
    
    // Stock Symbol (AAPL, padded with spaces to 8 bytes)
    data.extend_from_slice(b"AAPL    ");
    
    // Price (in 10^-4 dollars)
    let price: u64 = (170.45 * 10000.0) as u64;
    data.extend_from_slice(&price.to_be_bytes());
    
    data
}
