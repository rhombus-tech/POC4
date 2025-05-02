// Full end-to-end test of polynomial commit integration with TEE controller
// 
// This test verifies that polynomial commit operations work correctly through
// the full integration path, including the extension of an existing executor.

use std::sync::Arc;
use tokio::sync::RwLock;

use tee_controller::polynomial_integration::{PolynomialController, extend_tee_executor};
use tee_controller::enarx::controller::EnarxController;
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor, TeeType};

use ndarray::{Array2, ArrayView2};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use pasta_curves::Fp;
use ff::{Field, PrimeField};
use byteorder::{LittleEndian, ByteOrder};

// Helper function to create a random matrix
fn generate_random_matrix(rows: usize, cols: usize, seed: u64) -> Array2<Fp> {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let mut matrix = Array2::from_elem((rows, cols), Fp::ZERO);
    
    for i in 0..rows {
        for j in 0..cols {
            loop {
                let mut bytes = [0u8; 32];
                rng.fill(&mut bytes);
                bytes[31] &= 0x1F; // Ensure it's small enough for Fp
                
                if let Some(element) = Fp::from_repr_vartime(bytes.into()) {
                    matrix[[i, j]] = element;
                    break;
                }
            }
        }
    }
    
    matrix
}

// Encode an Fp element to bytes
fn encode_fp(element: &Fp) -> [u8; 32] {
    let bytes = element.to_repr();
    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes[..32]);
    result
}

// Encode a matrix to bytes
fn encode_matrix_to_bytes(matrix: &ArrayView2<Fp>) -> Vec<u8> {
    let rows = matrix.nrows();
    let cols = matrix.ncols();
    
    let mut buffer = Vec::with_capacity(8 + rows * cols * 32);
    
    // Write dimensions
    let mut dim_buffer = [0u8; 8];
    LittleEndian::write_u32(&mut dim_buffer[0..4], rows as u32);
    LittleEndian::write_u32(&mut dim_buffer[4..8], cols as u32);
    buffer.extend_from_slice(&dim_buffer);
    
    // Write elements
    for i in 0..rows {
        for j in 0..cols {
            buffer.extend_from_slice(&encode_fp(&matrix[[i, j]]));
        }
    }
    
    buffer
}

// Encode a vector to bytes
fn encode_vector_to_bytes(vec: &[Fp]) -> Vec<u8> {
    let len = vec.len();
    
    let mut buffer = Vec::with_capacity(4 + len * 32);
    
    // Write length
    let mut len_buffer = [0u8; 4];
    LittleEndian::write_u32(&mut len_buffer, len as u32);
    buffer.extend_from_slice(&len_buffer);
    
    // Write elements
    for element in vec {
        buffer.extend_from_slice(&encode_fp(element));
    }
    
    buffer
}

// Create a length-prefixed payload
fn create_length_prefixed_data(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(4 + data.len());
    
    // Add length prefix (u32 little-endian)
    let mut len_bytes = [0u8; 4];
    LittleEndian::write_u32(&mut len_bytes, data.len() as u32);
    result.extend_from_slice(&len_bytes);
    
    // Add the data
    result.extend_from_slice(data);
    result
}

#[tokio::test]
async fn test_polynomial_integration_with_extension() {
    // Enable testing mode
    std::env::set_var("RUNNING_TESTS", "1");
    
    // First, create a basic EnarxController (simulated)
    let enarx_controller = EnarxController::new(
        TeeType::SGX,
        "/tmp/enarx-test",
        true // simulate
    ).await.unwrap();
    
    // Create a polynomial controller
    let poly_controller = PolynomialController::new(TeeType::SGX).await.unwrap();
    
    // Combine them using extend_tee_executor
    let extended_controller = extend_tee_executor(enarx_controller, poly_controller);
    
    // Wrap in Arc<RwLock> to match real-world usage
    let executor = Arc::new(RwLock::new(extended_controller));
    
    // Generate test data
    let data_matrix = generate_random_matrix(4, 4, 12345);
    let g_matrix = generate_random_matrix(4, 4, 67890);
    let g_prime_t_matrix = generate_random_matrix(4, 4, 24680);
    
    // Encode matrices
    let data_bytes = encode_matrix_to_bytes(&data_matrix.view());
    let g_bytes = encode_matrix_to_bytes(&g_matrix.view());
    let g_prime_t_bytes = encode_matrix_to_bytes(&g_prime_t_matrix.view());
    
    // Combine data for input (use length-prefixed format for production compatibility)
    let mut combined_data = Vec::new();
    combined_data.extend_from_slice(&create_length_prefixed_data(&data_bytes));
    combined_data.extend_from_slice(&create_length_prefixed_data(&g_bytes));
    combined_data.extend_from_slice(&create_length_prefixed_data(&g_prime_t_bytes));
    
    // Create payload for secure_commit
    let payload = ExecutionPayload {
        input: combined_data,
        params: ExecutionParams {
            id_to: "test-contract".to_string(),
            function_call: "secure_commit".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
        region_id: Some("polynomial-region".to_string()),
        target_tee: None,
        tee_type: Some("SGX".to_string()),
        allow_fallback: None,
    };
    
    // Execute the secure_commit operation
    let commit_result = executor.read().await.execute(&payload).await.unwrap();
    assert!(!commit_result.result.is_empty(), "Expected non-empty commitment result");
    
    // Now perform a secure_open_at_point operation
    
    // Generate evaluation points
    let mut rng = ChaCha20Rng::seed_from_u64(13579);
    let mut point_r = Vec::with_capacity(4);
    let mut point_r_prime = Vec::with_capacity(4);
    
    for _ in 0..4 {
        // Generate points
        loop {
            let mut bytes = [0u8; 32];
            rng.fill(&mut bytes);
            bytes[31] &= 0x1F;
            
            if let Some(element) = Fp::from_repr_vartime(bytes.into()) {
                point_r.push(element);
                break;
            }
        }
        
        loop {
            let mut bytes = [0u8; 32];
            rng.fill(&mut bytes);
            bytes[31] &= 0x1F;
            
            if let Some(element) = Fp::from_repr_vartime(bytes.into()) {
                point_r_prime.push(element);
                break;
            }
        }
    }
    
    // Encode for open_at_point
    let point_r_bytes = encode_vector_to_bytes(&point_r);
    let point_r_prime_bytes = encode_vector_to_bytes(&point_r_prime);
    
    // Combine data
    let mut open_data = Vec::new();
    open_data.extend_from_slice(&create_length_prefixed_data(&data_bytes));
    open_data.extend_from_slice(&create_length_prefixed_data(&point_r_bytes));
    open_data.extend_from_slice(&create_length_prefixed_data(&point_r_prime_bytes));
    
    // Create payload for secure_open_at_point
    let open_payload = ExecutionPayload {
        input: open_data,
        params: ExecutionParams {
            id_to: "test-contract".to_string(),
            function_call: "secure_open_at_point".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
        region_id: Some("polynomial-region".to_string()),
        target_tee: None,
        tee_type: Some("SGX".to_string()),
        allow_fallback: None,
    };
    
    // Execute the open_at_point operation
    let open_result = executor.read().await.execute(&open_payload).await.unwrap();
    assert!(!open_result.result.is_empty(), "Expected non-empty opening result");
    
    println!("Polynomial commitment TEE integration test passed successfully!");
}
