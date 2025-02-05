use tonic::Request;
use std::error::Error;
use tee_controller::server::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::server::teeservice::ExecutionRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Read the WASM module
    let wasm_bytes = std::fs::read("target/wasm32-unknown-unknown/release/wasm_module.wasm")?;

    // Connect to the TEE service
    let mut client = TeeExecutionClient::connect("http://0.0.0.0:50051").await?;

    // Create execution request
    let request = Request::new(ExecutionRequest {
        id_to: "0".to_string(),
        function_call: "execute".to_string(),
        parameters: wasm_bytes,
        region_id: "default".to_string(),
        detailed_proof: true,
        expected_hash: vec![],
    });

    // Execute the request
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
