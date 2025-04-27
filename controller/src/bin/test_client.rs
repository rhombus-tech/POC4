use tonic::Request;
use tee_controller::proto::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::proto::teeservice::{
    ExecutionRequest, GetRegionsRequest, GetAttestationsRequest, DeployContractRequest
};
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TeeExecutionClient::connect("http://127.0.0.1:50051").await?;

    // Get available regions
    let regions_request = Request::new(GetRegionsRequest {});
    let regions_response = client.get_regions(regions_request).await?;
    println!("Available regions: {:?}", regions_response.into_inner());

    // Get attestations
    let attestations_request = Request::new(GetAttestationsRequest {
        region_id: "simulator".to_string(),
    });
    let attestations_response = client.get_attestations(attestations_request).await?;
    println!("Attestations: {:?}", attestations_response.into_inner());

    // First, deploy a test contract
    // Load the simple_add.wasm file from the test contracts directory
    let wasm_path = "/Users/talzisckind/Downloads/aristo-fresh 2/execution/controller/tests/contracts/simple_add/target/wasm32-unknown-unknown/release/simple_add.wasm";
    
    // Check if the file exists
    if !std::path::Path::new(wasm_path).exists() {
        println!("WASM file not found: {}. Trying to build it...", wasm_path);
        
        // Try to build the contract
        std::process::Command::new("cargo")
            .current_dir("/Users/talzisckind/Downloads/aristo-fresh 2/execution/controller/tests/contracts/simple_add")
            .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
            .status()?;
            
        if !std::path::Path::new(wasm_path).exists() {
            return Err("Failed to build WASM contract".into());
        }
    }
    
    let contract_bytes = fs::read(wasm_path)?;
    println!("Loaded WASM contract: {} bytes", contract_bytes.len());
    
    // Deploy the contract
    let deploy_request = Request::new(DeployContractRequest {
        contract_bytes: contract_bytes.clone(),
        region_id: "simulator".to_string(),
    });
    
    let deploy_response = client.deploy_contract(deploy_request).await?;
    let contract_address = deploy_response.into_inner().contract_id;
    println!("Deployed contract with address: {}", contract_address);

    // Execute contract with a method we know the simulator supports (add)
    println!("\nTesting add method (42 + 58):");
    let result = client
        .execute(Request::new(ExecutionRequest {
            id_to: contract_address.clone(),
            function_call: "add".to_string(),
            parameters: "42,58".as_bytes().to_vec(),
            region_id: "simulator".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        }))
        .await?;
    
    println!("Execution result: {:?}", result.into_inner());

    // Test an unsupported method
    println!("\nTesting unsupported method:");
    let unsupported_result = client
        .execute(Request::new(ExecutionRequest {
            id_to: contract_address.clone(),
            function_call: "unsupported".to_string(),
            parameters: vec![1, 2, 3],
            region_id: "simulator".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        }))
        .await;
    
    match unsupported_result {
        Ok(resp) => println!("Unexpected success: {:?}", resp.into_inner()),
        Err(err) => println!("Expected error: {}", err),
    }

    // Test invalid contract ID
    println!("\nTesting invalid contract ID:");
    let invalid_contract_result = client
        .execute(Request::new(ExecutionRequest {
            id_to: "non-existent-contract".to_string(),
            function_call: "add".to_string(),
            parameters: "1,2".as_bytes().to_vec(),
            region_id: "simulator".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        }))
        .await;
    
    match invalid_contract_result {
        Ok(resp) => println!("Unexpected success: {:?}", resp.into_inner()),
        Err(err) => println!("Expected error: {}", err),
    }

    Ok(())
}
