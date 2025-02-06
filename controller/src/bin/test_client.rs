use std::error::Error;
use tonic::transport::Channel;
use tonic::Request;
use hex;
use tee_controller::proto::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::proto::teeservice::{ExecutionRequest, CreateContractRequest, GetRegionsRequest, GetAttestationsRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TeeExecutionClient::connect("http://127.0.0.1:50051").await?;

    // Get available regions
    let regions_request = Request::new(GetRegionsRequest {});
    let regions_response = client.get_regions(regions_request).await?;
    println!("Available regions: {:?}", regions_response);

    // Get attestations
    let attestations_request = Request::new(GetAttestationsRequest {
        region_id: "simulator".to_string(),
    });
    let attestations_response = client.get_attestations(attestations_request).await?;
    println!("Attestations: {:?}", attestations_response);

    // Create contract
    let contract_code = include_bytes!("../../../target/wasm32-unknown-unknown/debug/tee_contract.wasm");
    let contract_code_bytes = if contract_code.len() > 4_000_000 {
        contract_code[..4_000_000].to_vec()
    } else {
        contract_code.to_vec()
    };
    let create_response = client
        .create_contract(Request::new(CreateContractRequest {
            wasm_code: contract_code_bytes,
            region_id: "simulator".to_string(),
        }))
        .await?;
    let contract_address = create_response.into_inner().address;
    println!("Contract created with address: {}", contract_address);

    // Execute contract
    let result = client
        .execute(Request::new(ExecutionRequest {
            id_to: contract_address.clone(),
            function_call: "execute".to_string(),
            parameters: vec![1, 2, 3],
            region_id: "simulator".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        }))
        .await?;
    println!("Execution result: {:?}", result.into_inner());

    Ok(())
}
