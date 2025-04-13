use aristo_client::itch::book::OrderBookReconstructor;
use aristo_client::itch::book::InterestFlag;
use aristo_client::itch::types::{ITCHMessage, MessageType, MessagePayload};
use aristo_client::itch::types::{NOIIMessage, RPIIMessage, LULDAuctionCollarMessage};
use aristo_client::protocol::WasmParameterHandler;
use aristo_client::error::Result;

fn test_noii_message() -> ITCHMessage {
    ITCHMessage {
        message_type: MessageType::NOII,
        timestamp: 1617289200000000000, // April 1, 2021
        stock: Some("AAPL".to_string()),
        payload: MessagePayload::NOII(NOIIMessage {
            paired_shares: 100000,
            imbalance_shares: 5000,
            imbalance_direction: b'B', // Buy
            far_price: 15000,
            near_price: 14975,
            current_reference_price: 14985,
            cross_type: b'O', // Opening
            price_variation_indicator: b'L', // Less
            stock: "AAPL".to_string(),
        }),
    }
}

fn test_rpii_message() -> ITCHMessage {
    ITCHMessage {
        message_type: MessageType::RPII,
        timestamp: 1617289200000000000, // April 1, 2021
        stock: Some("MSFT".to_string()),
        payload: MessagePayload::RPII(RPIIMessage {
            interest_flag: b'B', // Buy
            stock: "MSFT".to_string(),
        }),
    }
}

fn test_luld_message() -> ITCHMessage {
    ITCHMessage {
        message_type: MessageType::LULDAuctionCollar,
        timestamp: 1617289200000000000, // April 1, 2021
        stock: Some("GOOG".to_string()),
        payload: MessagePayload::LULDAuctionCollar(LULDAuctionCollarMessage {
            auction_collar_reference_price: 2500000,
            upper_auction_collar_price: 2750000,
            lower_auction_collar_price: 2250000,
            auction_collar_extension: 30,
            stock: "GOOG".to_string(),
        }),
    }
}

/// Test enhanced OrderBookReconstructor with new message types
#[test]
fn test_enhanced_order_book_reconstructor() -> Result<()> {
    let mut reconstructor = OrderBookReconstructor::new();
    
    // Process NOII message
    let noii_msg = test_noii_message();
    reconstructor.process_message(&noii_msg)?;
    
    // Process RPII message
    let rpii_msg = test_rpii_message();
    reconstructor.process_message(&rpii_msg)?;
    
    // Process LULD message
    let luld_msg = test_luld_message();
    reconstructor.process_message(&luld_msg)?;
    
    // Verify statistics
    let stats = reconstructor.get_statistics();
    assert_eq!(stats.get("messages_processed").unwrap(), &3);
    assert_eq!(stats.get("noii_messages").unwrap(), &1);
    assert_eq!(stats.get("rpii_messages").unwrap(), &1);
    assert_eq!(stats.get("luld_messages").unwrap(), &1);
    
    // Verify NOII data
    let noii_data = reconstructor.get_noii_data("AAPL").unwrap();
    assert_eq!(noii_data.paired_shares, 100000);
    assert_eq!(noii_data.imbalance_shares, 5000);
    
    // Verify RPII data
    let rpii_data = reconstructor.get_rpii_data("MSFT").unwrap();
    assert!(matches!(rpii_data.interest_flag, InterestFlag::Buy));
    
    // Verify LULD data
    let luld_data = reconstructor.get_luld_data("GOOG").unwrap();
    assert_eq!(luld_data.auction_collar_reference_price, 2500000);
    assert_eq!(luld_data.upper_auction_collar_price, 2750000);
    assert_eq!(luld_data.lower_auction_collar_price, 2250000);
    
    println!("Successfully tested enhanced OrderBookReconstructor!");
    Ok(())
}

/// Test WebAssembly parameter format handling
#[test]
fn test_wasm_parameter_handling() -> Result<()> {
    // Test length-prefixed format
    let test_data = b"AAPL|145.67|10000";
    let length_prefixed = WasmParameterHandler::to_length_prefixed(test_data);
    
    // Verify length prefix (should be 17 bytes in little-endian)
    assert_eq!(length_prefixed[0], 17);
    assert_eq!(length_prefixed[1], 0);
    assert_eq!(length_prefixed[2], 0);
    assert_eq!(length_prefixed[3], 0);
    
    // Parse it back
    let parsed = WasmParameterHandler::parse_parameters(&length_prefixed, None)?;
    assert_eq!(parsed, test_data);
    
    // Test direct format
    let direct = WasmParameterHandler::to_direct(test_data);
    assert_eq!(direct, test_data); // Direct format is unchanged
    
    // Parse direct with expected size
    let parsed_direct = WasmParameterHandler::parse_parameters(&direct, Some(test_data.len()))?;
    assert_eq!(parsed_direct, test_data);
    
    // Test market data parameter format
    let symbol = "MSFT";
    let price_data = b"267.50|5000";
    let market_param = WasmParameterHandler::create_market_data_parameter(symbol, price_data);
    
    // Extract the data
    let (extracted_symbol, extracted_price) = WasmParameterHandler::extract_market_data(&market_param)?;
    assert_eq!(extracted_symbol, symbol);
    assert_eq!(extracted_price, price_data);
    
    println!("Successfully tested WebAssembly parameter handling!");
    Ok(())
}

/// Run all enhanced market data tests
#[test]
fn run_all_enhanced_tests() -> Result<()> {
    test_enhanced_order_book_reconstructor()?;
    test_wasm_parameter_handling()?;
    println!("All enhanced market data tests passed!");
    Ok(())
}
