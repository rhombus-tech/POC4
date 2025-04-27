/*!
 * ITCH Protocol Integration Test using Enarx Simulation Mode
 * 
 * Demonstrates how to use the ITCH protocol implementation with
 * the Aristo TEE mesh for secure market data processing.
 */

use tee_controller::enarx::{EnarxController};
use tee_interface::types::{RegionId, ContractId, ExecutionResult, ParameterFormat, TeeType};
use tee_interface::{ExecutionPayload, TeeError};
use aristo_client::itch::{ITCHParser, OrderBookReconstructor};
use aristo_client::protocol::ParameterFormat as ClientParameterFormat;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use serde_json;

const MARKET_DATA_CONTRACT_PATH: &str = 
    "tests/contracts/market_data_consumer/target/wasm32-unknown-unknown/release/market_data_consumer.wasm";

/// Test ITCH market data processing through the Enarx simulation mode
#[tokio::test]
async fn test_itch_enarx_simulation() {
    // Initialize the test controller in simulation mode
    let controller = init_enarx_simulator().await;
    
    // Process sample ITCH data to create an order book
    let (symbol, order_book) = process_sample_itch_data();
    println!("Processed order book for {}: {} bids, {} asks", 
        symbol, order_book.bids.len(), order_book.asks.len());
    
    // Convert order book to JSON for contract consumption
    let order_book_json = serde_json::to_vec(&order_book).expect("Failed to serialize order book");
    
    // Add length prefix for contract execution (little-endian)
    let mut parameter_data = Vec::new();
    parameter_data.extend_from_slice(&(order_book_json.len() as u32).to_le_bytes());
    parameter_data.extend_from_slice(&order_book_json);
    
    // Create an execution payload for testing
    let region_id = "test-region-1";
    let function_name = "calculate_metrics";
    
    // Use the payload directly since we're in simulation mode
    let payload = ExecutionPayload {
        contract_id: "test-contract-id".to_string(),
        function: function_name.to_string(),
        parameters: parameter_data,
        parameter_format: ParameterFormat::LengthPrefixed,
        region_id: RegionId::new(region_id),
    };
    
    // Execute the payload in simulation mode
    let result = controller.execute(&payload).await;
    
    match result {
        Ok(execution_result) => {
            println!("\nContract execution successful!");
            
            // Parse the result (length-prefixed JSON)
            if execution_result.data.len() > 4 {
                // Extract length from first 4 bytes (little-endian)
                let len_bytes = [
                    execution_result.data[0],
                    execution_result.data[1],
                    execution_result.data[2],
                    execution_result.data[3],
                ];
                let len = u32::from_le_bytes(len_bytes) as usize;
                
                // Parse the JSON data
                if 4 + len <= execution_result.data.len() {
                    let json_data = &execution_result.data[4..4+len];
                    let metrics: serde_json::Value = serde_json::from_slice(json_data)
                        .expect("Failed to parse result JSON");
                    
                    println!("\n=== Market Metrics from TEE Simulation ===");
                    println!("Symbol: {}", metrics["symbol"]);
                    println!("Midpoint Price: ${:.4}", metrics["midpoint_price"].as_f64().unwrap_or(0.0));
                    println!("Bid/Ask Spread: ${:.4}", metrics["bid_ask_spread"].as_f64().unwrap_or(0.0));
                    println!("Spread Percentage: {:.4}%", metrics["spread_percentage"].as_f64().unwrap_or(0.0));
                    println!("Market Depth (USD): ${:.2}", metrics["market_depth_usd"].as_f64().unwrap_or(0.0));
                    println!("Timestamp: {}", metrics["timestamp"]);
                    
                    println!("\nSimulation test completed successfully!");
                } else {
                    println!("Invalid result length: expected {} bytes, got {} bytes", 
                        len, execution_result.data.len() - 4);
                }
            } else {
                println!("Result too short to contain length prefix");
            }
        },
        Err(e) => {
            println!("Contract execution failed: {:?}", e);
        }
    }
}

/// Initialize Enarx controller in simulation mode
async fn init_enarx_simulator() -> EnarxController {
    // Create a new Enarx controller in simulation mode
    let config_dir = "./enarx_config";
    let simulation = true;
    let tee_type = TeeType::Sgx;
    
    let controller = EnarxController::new(tee_type, config_dir, simulation)
        .await
        .expect("Failed to create Enarx controller");
    
    controller
}

/// Process sample ITCH data to create an order book
fn process_sample_itch_data() -> (String, aristo_client::itch::OrderBook) {
    // Create a new ITCH parser and order book reconstructor
    let mut parser = ITCHParser::new();
    let mut book_reconstructor = OrderBookReconstructor::new();
    
    // Create some sample orders for testing
    let sample_messages = create_sample_messages();
    
    // Process each message
    for message_data in sample_messages {
        let message = parser.parse_message(&message_data)
            .expect("Failed to parse message");
            
        book_reconstructor.process_message(&message)
            .expect("Failed to process message");
    }
    
    // Get the order book for AAPL
    let symbol = "AAPL".to_string();
    let order_book = book_reconstructor.get_order_book(&symbol)
        .expect("Failed to get order book");
        
    (symbol, order_book)
}

/// Create sample ITCH messages for testing
fn create_sample_messages() -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    
    // System Event Message
    messages.push(create_system_event_message());
    
    // Add Order Messages for AAPL
    messages.push(create_add_order(1001, 'B', 100, "AAPL", 15000)); // $1.50
    messages.push(create_add_order(1002, 'B', 200, "AAPL", 14900)); // $1.49
    messages.push(create_add_order(1003, 'B', 300, "AAPL", 15100)); // $1.51
    messages.push(create_add_order(2001, 'S', 150, "AAPL", 15200)); // $1.52
    messages.push(create_add_order(2002, 'S', 250, "AAPL", 15300)); // $1.53
    messages.push(create_add_order(2003, 'S', 350, "AAPL", 15250)); // $1.525
    
    // Order Executed Message
    messages.push(create_order_executed(1001, 50)); // Partially execute order 1001
    
    messages
}

/// Create a System Event message
fn create_system_event_message() -> Vec<u8> {
    let message_type = b'S'; // System Event
    let timestamp = 1234567890u64.to_be_bytes();
    let event_code = b'O'; // Start of Messages
    
    let mut message_data = vec![message_type[0]];
    message_data.extend_from_slice(&timestamp);
    message_data.extend_from_slice(&event_code);
    
    message_data
}

/// Create an Add Order message
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
    
    let mut message_data = vec![message_type[0]];
    message_data.extend_from_slice(&timestamp);
    message_data.extend_from_slice(&order_ref);
    message_data.extend_from_slice(&buy_sell);
    message_data.extend_from_slice(&shares);
    message_data.extend_from_slice(&stock_bytes);
    message_data.extend_from_slice(&price);
    
    message_data
}

/// Create an Order Executed message
fn create_order_executed(order_ref: u64, executed_shares: u32) -> Vec<u8> {
    let message_type = b'E'; // Order Executed
    let timestamp = 9876543210u64.to_be_bytes();
    let order_ref = order_ref.to_be_bytes();
    let shares = executed_shares.to_be_bytes();
    let match_number = 12345u64.to_be_bytes();
    
    let mut message_data = vec![message_type[0]];
    message_data.extend_from_slice(&timestamp);
    message_data.extend_from_slice(&order_ref);
    message_data.extend_from_slice(&shares);
    message_data.extend_from_slice(&match_number);
    
    message_data
}
