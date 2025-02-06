use tonic::Request;
use std::error::Error;
use tee_controller::server::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::server::teeservice::ExecutionRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create test client
    let mut client = TeeExecutionClient::connect("http://[::1]:50051").await?;

    // Create test payload
    let request = ExecutionRequest {
        id_to: "1".to_string(),
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
