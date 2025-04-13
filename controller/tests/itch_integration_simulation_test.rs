/*!
 * ITCH Protocol Integration Test - Simulation Mode
 * 
 * This integration test demonstrates the NASDAQ ITCH protocol integration with
 * the Aristo TEE mesh architecture in simulation mode.
 */

// Import client types with market-data feature enabled
use aristo_client::{AristoClient, ParameterFormat};
use aristo_controller::enarx::EnarxController;
use aristo_controller::tee::{ExecutionPayload, ContractType};
use aristo_interface::error::TeeError;

// Import ITCH types directly from client crate
use aristo_client::itch::types::{
    Message, MessageType, MessagePayload, 
    AddOrderMessage, OrderExecutedMessage, OrderDeleteMessage, BuySellIndicator
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::File;
use std::io::Read;

// Test sample ITCH messages in simulation mode
#[tokio::test]
async fn test_itch_processing_simulation() -> Result<(), TeeError> {
    // Initialize the TEE controller in simulation mode
    let controller = EnarxController::new_simulation();
    
    // Load the market data consumer contract
    let contract_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/contracts/market_data_consumer/target/wasm32-unknown-unknown/release/market_data_consumer.wasm");
    
    let mut contract_bytes = Vec::new();
    File::open(contract_path)
        .expect("Failed to open contract file")
        .read_to_end(&mut contract_bytes)
        .expect("Failed to read contract file");
    
    // Deploy the contract
    let contract_id = controller.deploy(
        &contract_bytes,
        ContractType::Wasm,
        "market_data_consumer",
    ).await?;
    
    println!("Deployed market data consumer contract: {}", contract_id);
    
    // Create sample ITCH messages
    let sample_messages = create_sample_itch_messages();
    
    // Initialize the Aristo client with simulation features
    let client_config = aristo_client::ClientConfig {
        regions: HashMap::new(),
        attestation: None,
        external_apis: HashMap::new(),
    };
    
    let client = AristoClient::new(client_config);
    
    // Create an ITCH client
    let itch_client = client.itch();
    
    // Process each sample message
    for (i, message) in sample_messages.iter().enumerate() {
        println!("Processing sample message {}", i + 1);
        
        // Transform the message for contract execution with length-prefixed format
        let params = itch_client.prepare_for_contract(message, ParameterFormat::LengthPrefixed)
            .expect("Failed to prepare parameters");
        
        // Execute the contract with the ITCH data
        let payload = ExecutionPayload {
            contract_id: contract_id.clone(),
            function: "process_itch_data".to_string(),
            parameters: params,
            results_format: ParameterFormat::LengthPrefixed,
        };
        
        let result = controller.execute(&payload).await?;
        
        // Parse the results
        let result_json = if result.result.len() >= 4 {
            // Extract length-prefixed data
            let mut len_bytes = [0u8; 4];
            len_bytes.copy_from_slice(&result.result[0..4]);
            let len = u32::from_le_bytes(len_bytes) as usize;
            
            if len <= result.result.len() - 4 {
                let json_data = &result.result[4..4+len];
                String::from_utf8_lossy(json_data).to_string()
            } else {
                String::from_utf8_lossy(&result.result).to_string()
            }
        } else {
            String::from_utf8_lossy(&result.result).to_string()
        };
        
        println!("Contract execution result: {}", result_json);
    }
    
    // Run the same test with direct format parameters
    println!("\nTesting with direct parameter format");
    
    for (i, message) in sample_messages.iter().enumerate() {
        println!("Processing sample message {} with direct format", i + 1);
        
        // Transform the message for contract execution with direct format
        let params = itch_client.prepare_for_contract(message, ParameterFormat::Direct)
            .expect("Failed to prepare parameters");
        
        // Execute the contract with the ITCH data
        let payload = ExecutionPayload {
            contract_id: contract_id.clone(),
            function: "process_itch_data".to_string(),
            parameters: params,
            results_format: ParameterFormat::Direct,
        };
        
        let result = controller.execute(&payload).await?;
        
        // Parse the results (direct format)
        let result_json = String::from_utf8_lossy(&result.result).to_string();
        println!("Contract execution result (direct): {}", result_json);
    }
    
    Ok(())
}

// Create sample ITCH messages for testing
fn create_sample_itch_messages() -> Vec<Message> {
    let mut messages = Vec::new();
    
    // Add Order Message (Type A)
    messages.push(Message {
        message_type: MessageType::AddOrder,
        stock: "AAPL".to_string(),
        timestamp: 1649754000000000000, // Nanoseconds since Unix epoch
        payload: MessagePayload::AddOrder(AddOrderMessage {
            order_reference_number: 12345678,
            buy_sell_indicator: BuySellIndicator::Buy,
            shares: 100,
            stock: "AAPL".to_string(),
            price: 170.45,
        }),
    });
    
    // Order Executed Message (Type E)
    messages.push(Message {
        message_type: MessageType::OrderExecuted,
        stock: "AAPL".to_string(),
        timestamp: 1649754001000000000,
        payload: MessagePayload::OrderExecuted(OrderExecutedMessage {
            order_reference_number: 12345678,
            executed_shares: 50,
            match_number: 987654321,
        }),
    });
    
    // Order Delete Message (Type D)
    messages.push(Message {
        message_type: MessageType::OrderDelete,
        stock: "AAPL".to_string(),
        timestamp: 1649754002000000000,
        payload: MessagePayload::OrderDelete(OrderDeleteMessage {
            order_reference_number: 12345678,
        }),
    });
    
    messages
}
