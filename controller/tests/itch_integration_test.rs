/*!
 * ITCH Protocol Integration Test
 * 
 * Demonstrates how to use the ITCH protocol implementation with
 * the Aristo TEE mesh for secure market data processing.
 */

use tee_controller::controller::{
    Controller, ControllerClient, ControllerConfig, DeploymentMode, SimulationConfig,
};
use tee_controller::contract::ContractManager;
use tee_interface::types::{RegionId, ContractId, ExecutionResult, ParameterFormat};
use aristo_client::itch::{ITCHClient, ITCHParser, OrderBookReconstructor};
use aristo_client::protocol::ParameterFormat as ClientParameterFormat;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

const MARKET_DATA_CONTRACT_PATH: &str = 
    "tests/contracts/market_data_consumer/target/wasm32-unknown-unknown/release/market_data_consumer.wasm";

/// Test ITCH market data processing through the TEE mesh
#[tokio::test]
async fn test_itch_market_data_processing() {
    // Initialize the test controller in simulation mode
    let controller = init_test_controller().await;
    let contract_id = deploy_market_data_contract(&controller).await;
    
    // Process sample ITCH data to create an order book
    let (symbol, order_book) = process_sample_itch_data().await;
    println!("Processed order book for {}: {} bids, {} asks", 
        symbol, order_book.bids.len(), order_book.asks.len());
        
    // Transform order book for contract execution with length-prefixed format
    let client = ITCHClient::new();
    let parameter_data = client.transform_order_book(&symbol, ClientParameterFormat::LengthPrefixed)
        .await.expect("Failed to transform order book");
    
    // Execute the contract through the TEE
    let result = controller.execute_contract(
        &contract_id,
        "calculate_metrics",
        parameter_data.clone(),
        ParameterFormat::LengthPrefixed,
    ).await.expect("Contract execution failed");
    
    // Parse and display the metrics
    let metrics: serde_json::Value = parse_contract_result(&result);
    println!("\n=== Market Metrics from TEE ===");
    println!("Symbol: {}", metrics["symbol"]);
    println!("Midpoint Price: ${:.4}", metrics["midpoint_price"].as_f64().unwrap_or(0.0));
    println!("Bid/Ask Spread: ${:.4}", metrics["bid_ask_spread"].as_f64().unwrap_or(0.0));
    println!("Spread Percentage: {:.4}%", metrics["spread_percentage"].as_f64().unwrap_or(0.0));
    println!("Market Depth (USD): ${:.2}", metrics["market_depth_usd"].as_f64().unwrap_or(0.0));
    println!("Timestamp: {}", metrics["timestamp"]);
    
    // Test with direct format
    let direct_parameter_data = client.transform_order_book(&symbol, ClientParameterFormat::Direct)
        .await.expect("Failed to transform order book");
        
    let direct_result = controller.execute_contract(
        &contract_id,
        "calculate_metrics",
        direct_parameter_data,
        ParameterFormat::Direct,
    ).await.expect("Contract execution failed");
    
    let direct_metrics: serde_json::Value = parse_contract_result(&direct_result);
    
    // Verify both formats produce the same results
    assert_eq!(
        metrics["midpoint_price"].as_f64().unwrap_or(0.0),
        direct_metrics["midpoint_price"].as_f64().unwrap_or(0.0),
        "Results from different parameter formats should match"
    );
}

/// Initialize a test controller in simulation mode
async fn init_test_controller() -> Arc<Controller> {
    let config = ControllerConfig {
        deployment_mode: DeploymentMode::Simulation(SimulationConfig {
            region_id: RegionId::new("test-region-1"),
            attestation_enabled: false,
            sync_delay_ms: 10,
        }),
        contract_dir: PathBuf::from("./contracts"),
        peer_refresh_interval: Duration::from_secs(1),
        grpc_port: 0, // Use any available port
        ..Default::default()
    };
    
    let controller = Controller::new(config).await.expect("Failed to create controller");
    Arc::new(controller)
}

/// Deploy the market data consumer contract
async fn deploy_market_data_contract(controller: &Arc<Controller>) -> ContractId {
    let contract_path = PathBuf::from(MARKET_DATA_CONTRACT_PATH);
    
    // Read contract bytecode
    let mut file = File::open(&contract_path)
        .await
        .expect("Failed to open contract file");
        
    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode)
        .await
        .expect("Failed to read contract file");
        
    // Deploy the contract
    let contract_id = controller
        .deploy_contract(bytecode)
        .await
        .expect("Failed to deploy contract");
        
    println!("Deployed market data contract with ID: {}", contract_id);
    contract_id
}

/// Process sample ITCH data to create an order book
async fn process_sample_itch_data() -> (String, aristo_client::itch::OrderBook) {
    // Create a new ITCH client
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

/// Parse contract execution result
fn parse_contract_result(result: &ExecutionResult) -> serde_json::Value {
    // Contract returns length-prefixed JSON
    let result_data = &result.data;
    
    // Extract the length prefix (first 4 bytes)
    let len_bytes = [
        result_data[0],
        result_data[1],
        result_data[2],
        result_data[3],
    ];
    let len = u32::from_le_bytes(len_bytes) as usize;
    
    // Parse the JSON data
    let json_data = &result_data[4..4+len];
    serde_json::from_slice(json_data).expect("Failed to parse result JSON")
}
