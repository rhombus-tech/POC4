use std::error::Error;
use tonic::{Request, Status};
use tee_controller::server::teeservice::tee_execution_client::TeeExecutionClient;
use tee_controller::server::teeservice::{self, ExecutionRequest, ExecutionResult, TeeAttestation};

// Mock WASM module header - this is just for testing, not a real WASM module
const MOCK_WASM: &[u8] = &[0x00, 0x61, 0x73, 0x6D];

async fn setup_client() -> Result<TeeExecutionClient<tonic::transport::Channel>, Box<dyn Error>> {
    let client = TeeExecutionClient::connect("http://0.0.0.0:50051").await?;
    Ok(client)
}

/// Create a test request with mock WASM code
/// Note: The simulator doesn't actually execute WASM code, it just simulates an 'add' function
fn create_request(id: &str, function: &str, params: &[u8]) -> Request<ExecutionRequest> {
    let mut input = MOCK_WASM.to_vec();
    input.push(b',');
    input.extend_from_slice(params);
    
    Request::new(ExecutionRequest {
        id_to: id.to_string(),
        function_call: function.to_string(),
        parameters: input,
        region_id: "default".to_string(),
        detailed_proof: true,
        expected_hash: vec![],
    })
}

fn verify_attestation(attestation: &TeeAttestation) {
    assert!(!attestation.enclave_id.is_empty(), "Enclave ID should not be empty");
    assert!(!attestation.measurement.is_empty(), "Measurement should not be empty");
    assert!(!attestation.timestamp.is_empty(), "Timestamp should not be empty");
    assert!(!attestation.signature.is_empty(), "Signature should not be empty");
    assert!(!attestation.enclave_type.is_empty(), "Enclave type should not be empty");
}

/// Test the mock add function
/// Note: This is testing the simulator's mock implementation, not actual WASM execution
#[tokio::test]
async fn test_mock_add() -> Result<(), Box<dyn Error>> {
    let mut client = setup_client().await?;
    let request = create_request("1", "add", b"1,2");
    let response = client.execute(request).await?;
    let result = response.into_inner();

    // Verify response
    assert!(!result.result.is_empty(), "Execution result should not be empty");
    assert!(result.syscall_count >= 0, "Syscall count should be non-negative");

    // Verify attestations
    assert!(!result.attestations.is_empty(), "Should have at least one attestation");
    for attestation in result.attestations {
        verify_attestation(&attestation);
    }

    Ok(())
}

/// Test error handling for unsupported methods in the mock simulator
#[tokio::test]
async fn test_mock_unsupported_method() -> Result<(), Box<dyn Error>> {
    let mut client = setup_client().await?;
    let request = create_request("2", "unsupported_method", b"1,2");
    
    match client.execute(request).await {
        Ok(_) => panic!("Expected error for unsupported method"),
        Err(status) => {
            assert_eq!(status.code(), tonic::Code::Internal);
            // Check error contains key information without exact string match
            let msg = status.message();
            assert!(msg.contains("not found"), "Error should indicate function not found");
            assert!(msg.contains("unsupported_method"), "Error should mention the function name");
        }
    }

    Ok(())
}

/// Test error handling for invalid parameters in the mock simulator
#[tokio::test]
async fn test_mock_invalid_parameters() -> Result<(), Box<dyn Error>> {
    let mut client = setup_client().await?;
    let request = create_request("3", "add", b"1"); // Only one parameter
    
    match client.execute(request).await {
        Ok(_) => panic!("Expected error for wrong number of parameters"),
        Err(status) => {
            assert_eq!(status.code(), tonic::Code::Internal);
            // Check error contains key information without exact string match
            let msg = status.message();
            assert!(msg.contains("Expected 2 parameters"), "Error should mention expected parameter count");
            assert!(msg.contains("add"), "Error should mention the function name");
        }
    }

    Ok(())
}
