use log::debug;
use ndarray::{Array2, ArrayView2};
use pasta_curves::Fp;
use tee_controller::polynomial_integration::PolynomialController;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeType};
use tee_interface::ExecutionParams;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::env;
use std::io::Write;
use byteorder::{ByteOrder, LittleEndian};
use ff::{Field, PrimeField};

// Generate a random matrix of Fp elements with specified dimensions
fn generate_random_matrix(rows: usize, cols: usize, seed: u64) -> Array2<Fp> {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    // Create matrix with Fp::ZERO elements instead of using zeros() which requires num_traits::Zero
    let mut matrix = Array2::from_elem((rows, cols), Fp::ZERO);
    
    for i in 0..rows {
        for j in 0..cols {
            // Generate random bytes and try to convert to Fp
            loop {
                let mut bytes = [0u8; 32];
                rng.fill(&mut bytes);
                
                // Make sure it's a valid Fp element (less than the modulus)
                bytes[31] &= 0x1F; // Ensure it's small enough
                
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

// Encode a matrix of Fp elements to bytes
fn encode_matrix_to_bytes(matrix: &ArrayView2<Fp>) -> Vec<u8> {
    let rows = matrix.nrows();
    let cols = matrix.ncols();
    
    // Buffer to hold the serialized matrix
    let mut buffer = Vec::with_capacity(8 + rows * cols * 32);
    
    // Write dimensions (rows and cols as u32)
    let mut dim_buffer = [0u8; 8];
    LittleEndian::write_u32(&mut dim_buffer[0..4], rows as u32);
    LittleEndian::write_u32(&mut dim_buffer[4..8], cols as u32);
    buffer.extend_from_slice(&dim_buffer);
    
    // Write matrix elements in row-major order
    for i in 0..rows {
        for j in 0..cols {
            buffer.extend_from_slice(&encode_fp(&matrix[[i, j]]));
        }
    }
    
    buffer
}

// Encode a vector of Fp elements to bytes
fn encode_vector_to_bytes(vec: &[Fp]) -> Vec<u8> {
    let len = vec.len();
    
    // Buffer to hold the serialized vector
    let mut buffer = Vec::with_capacity(4 + len * 32);
    
    // Write length as u32
    let mut len_buffer = [0u8; 4];
    LittleEndian::write_u32(&mut len_buffer, len as u32);
    buffer.extend_from_slice(&len_buffer);
    
    // Write vector elements
    for element in vec {
        buffer.extend_from_slice(&encode_fp(element));
    }
    
    buffer
}

// Package data for a secure_commit operation
fn prepare_secure_commit_data(data: &ArrayView2<Fp>, g: &ArrayView2<Fp>, g_prime_t: &ArrayView2<Fp>) -> Vec<u8> {
    let mut buffer = Vec::new();
    
    // Encode the data matrix
    let data_bytes = encode_matrix_to_bytes(data);
    buffer.extend_from_slice(&data_bytes);
    
    // Encode the g matrix
    let g_bytes = encode_matrix_to_bytes(g);
    buffer.extend_from_slice(&g_bytes);
    
    // Encode the g_prime_t matrix
    let g_prime_t_bytes = encode_matrix_to_bytes(g_prime_t);
    buffer.extend_from_slice(&g_prime_t_bytes);
    
    buffer
}

// Package data for a secure_open_at_point operation
fn prepare_secure_open_at_point_data(data: &ArrayView2<Fp>, point_r: &[Fp], point_r_prime: &[Fp]) -> Vec<u8> {
    let mut buffer = Vec::new();
    
    // Encode the data matrix
    let data_bytes = encode_matrix_to_bytes(data);
    buffer.extend_from_slice(&data_bytes);
    
    // Encode the point_r vector
    let point_r_bytes = encode_vector_to_bytes(point_r);
    buffer.extend_from_slice(&point_r_bytes);
    
    // Encode the point_r_prime vector
    let point_r_prime_bytes = encode_vector_to_bytes(point_r_prime);
    buffer.extend_from_slice(&point_r_prime_bytes);
    
    buffer
}

// Helper function to create length-prefixed format data
fn create_length_prefixed(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len() + 4);
    let len = data.len() as u32;
    result.extend_from_slice(&len.to_le_bytes());
    result.extend_from_slice(data);
    result
}

// Helper function to create malicious data (3.5B byte attack)
fn create_malicious_data() -> Vec<u8> {
    let mut result = Vec::with_capacity(8);
    let huge_size = 0xD0BD_0000u32; // ~3.5B bytes
    result.extend_from_slice(&huge_size.to_le_bytes());
    result.extend_from_slice(&[1, 2, 3, 4]); // Some actual data
    result
}

#[tokio::test]
async fn test_polynomial_controller_secure_commit() {
    // Set the RUNNING_TESTS environment variable for the TeeIntegration
    env::set_var("RUNNING_TESTS", "1");

    // Create the controller
    let controller = PolynomialController::new(TeeType::SGX).await.expect("Failed to create controller");
    
    // Create real test matrices with proper dimensions
    let rows = 4; // Power of 2 for polynomial operations
    let cols = 4;
    
    // Generate random matrices with different seeds
    let data_matrix = generate_random_matrix(rows, cols, 42);
    let g_matrix = generate_random_matrix(rows, cols, 43);
    let g_prime_t_matrix = generate_random_matrix(rows, cols, 44);
    
    // Properly encode the data for a real secure_commit operation
    let combined_data = prepare_secure_commit_data(
        &data_matrix.view(),
        &g_matrix.view(),
        &g_prime_t_matrix.view()
    );
    
    // Direct format test
    let payload = ExecutionPayload {
        input: combined_data.clone(),
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
    
    let result = controller.execute(&payload).await;
    assert!(result.is_ok(), "Direct format secure_commit should succeed");
    
    // Length-prefixed format test
    let length_prefixed_data = create_length_prefixed(&combined_data);
    
    let payload_prefixed = ExecutionPayload {
        input: length_prefixed_data,
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
    
    let result_prefixed = controller.execute(&payload_prefixed).await;
    assert!(result_prefixed.is_ok(), "Length-prefixed format secure_commit should succeed");
    
    // Test with malicious input
    let malicious_data = create_malicious_data();
    
    let payload_malicious = ExecutionPayload {
        input: malicious_data,
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
    
    let result_malicious = controller.execute(&payload_malicious).await;
    assert!(result_malicious.is_err(), "Malicious input should be rejected");
}

#[tokio::test]
async fn test_polynomial_controller_secure_open_at_point() {
    // Set the RUNNING_TESTS environment variable for the TeeIntegration
    env::set_var("RUNNING_TESTS", "1");

    // Create the controller
    let controller = PolynomialController::new(TeeType::SGX).await.expect("Failed to create controller");
    
    // Create real test data with proper dimensions
    let rows = 4; // Power of 2 for polynomial operations
    let cols = 4;
    
    // Generate random matrices and vectors with different seeds
    let data_matrix = generate_random_matrix(rows, cols, 42);
    
    // Generate random vectors for evaluation points
    let mut rng = ChaCha20Rng::seed_from_u64(45);
    let mut point_r = Vec::with_capacity(rows.next_power_of_two().trailing_zeros() as usize);
    let mut point_r_prime = Vec::with_capacity(cols.next_power_of_two().trailing_zeros() as usize);
    
    // Fill with random Fp elements
    for _ in 0..point_r.capacity() {
        loop {
            let mut bytes = [0u8; 32];
            rng.fill(&mut bytes);
            bytes[31] &= 0x1F; // Ensure it's small enough
            
            if let Some(element) = Fp::from_repr_vartime(bytes.into()) {
                point_r.push(element);
                break;
            }
        }
    }
    
    for _ in 0..point_r_prime.capacity() {
        loop {
            let mut bytes = [0u8; 32];
            rng.fill(&mut bytes);
            bytes[31] &= 0x1F; // Ensure it's small enough
            
            if let Some(element) = Fp::from_repr_vartime(bytes.into()) {
                point_r_prime.push(element);
                break;
            }
        }
    }
    
    // Properly encode the data for a real secure_open_at_point operation
    let combined_data = prepare_secure_open_at_point_data(
        &data_matrix.view(),
        &point_r,
        &point_r_prime
    );
    
    // Direct format test
    let payload = ExecutionPayload {
        input: combined_data.clone(),
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
    
    let result = controller.execute(&payload).await;
    assert!(result.is_ok(), "Direct format secure_open_at_point should succeed");
    
    // Length-prefixed format test
    let length_prefixed_data = create_length_prefixed(&combined_data);
    
    let payload_prefixed = ExecutionPayload {
        input: length_prefixed_data,
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
    
    let result_prefixed = controller.execute(&payload_prefixed).await;
    assert!(result_prefixed.is_ok(), "Length-prefixed format secure_open_at_point should succeed");
    
    // Test with malicious input
    let malicious_data = create_malicious_data();
    
    let payload_malicious = ExecutionPayload {
        input: malicious_data,
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
    
    let result_malicious = controller.execute(&payload_malicious).await;
    assert!(result_malicious.is_err(), "Malicious input should be rejected");
}
