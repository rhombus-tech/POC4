use tonic::Request;
use std::error::Error;
use tee_controller::server::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::server::teeservice::ExecutionRequest;

// Mock WASM module header - this is just for testing, not a real WASM module
const MOCK_WASM: &[u8] = &[0x00, 0x61, 0x73, 0x6D];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create input - include mock WASM code and parameters
    let mut input = MOCK_WASM.to_vec();
    input.push(b',');
    
    // Parameters for addition - the simulator will add these two numbers
    input.extend_from_slice(b"1,2");  // 1 + 2 = 3

    // Connect to the TEE service
    let mut client = TeeExecutionClient::connect("http://0.0.0.0:50051").await?;

    // Create execution request
    let request = Request::new(ExecutionRequest {
        id_to: "1".to_string(),
        function_call: "add".to_string(),  // The simulator only supports "add"
        parameters: input,
        region_id: "default".to_string(), // Use the default region that the service adds
        detailed_proof: true,
        expected_hash: vec![],
    });

    // Execute the request
    println!("Sending request to TEE service...");
    let response = client.execute(request).await?;
    let result = response.into_inner();

    println!("Execution Result:");
    println!("Timestamp: {}", result.timestamp);
    println!("Result: {:?}", result.result);
    println!("State Hash: {:?}", result.state_hash);
    println!("Execution Time: {}ms", result.execution_time);
    println!("Memory Used: {} bytes", result.memory_used);
    println!("Syscall Count: {}", result.syscall_count);
    println!("\nAttestations:");
    for attestation in result.attestations {
        println!("  Enclave ID: {:?}", attestation.enclave_id);
        println!("  Measurement: {:?}", attestation.measurement);
        println!("  Timestamp: {}", attestation.timestamp);
        println!("  Signature: {:?}", attestation.signature);
        println!("  Enclave Type: {}", attestation.enclave_type);
    }

    Ok(())
}
