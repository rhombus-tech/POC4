use tonic::Request;
use std::error::Error;
use tee_controller::server::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::server::teeservice::{ExecutionRequest, CreateContractRequest};
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create test client
    let mut client = TeeExecutionClient::connect("http://127.0.0.1:50051").await?;

    // First, deploy the contract
    let wasm_code = fs::read("tests/contracts/simple_add/target/wasm32-unknown-unknown/debug/simple_add.wasm")?;
    let create_request = CreateContractRequest {
        wasm_code,
        region_id: "default".to_string(),
    };
    let create_response = client.create_contract(Request::new(create_request)).await?;
    println!("Contract created with address: {:?}", create_response.get_ref().address);

    // Create test payload using the created contract address
    let request = ExecutionRequest {
        id_to: create_response.get_ref().address.clone(),
        function_call: "add".to_string(),
        parameters: vec![1, 2, 3],  // Test parameters
        region_id: "default".to_string(),
        detailed_proof: false,
        expected_hash: vec![],  // No expected hash
    };

    // Execute payload
    let response = client.execute(Request::new(request)).await?;
    println!("RESPONSE={:?}", response);

    Ok(())
}
