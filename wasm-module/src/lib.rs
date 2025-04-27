#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use wasmlanche::{
    types::WasmlAddress,
    context::Context,
};

use borsh::{BorshDeserialize, BorshSerialize};
use chrono;
use log;
use sha2::{Sha256, Digest};
use std::alloc::Layout;
use thiserror::Error;

mod computation;
pub use computation::*;

/// Errors that can occur during WASM execution
#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Failed to deserialize payload: {0}")]
    DeserializationError(String),

    #[error("Computation failed: {0}")]
    ComputationError(String),

    #[error("Failed to serialize result: {0}")]
    SerializationError(String),

    #[error("Failed to store result: {0}")]
    StorageError(String),
}

impl From<String> for ExecutionError {
    fn from(err: String) -> Self {
        ExecutionError::ComputationError(err)
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logger");
}

// Required memory export for Hypersdk
#[no_mangle]
static mut MEMORY: [u8; 0] = [];

// Required memory allocation functions
#[no_mangle]
pub unsafe fn alloc(size: i32) -> *mut u8 {
    let layout = Layout::from_size_align(size as usize, 8).unwrap();
    std::alloc::alloc(layout)
}

#[no_mangle]
pub unsafe fn dealloc(ptr: *mut u8, size: i32) {
    let layout = Layout::from_size_align(size as usize, 8).unwrap();
    std::alloc::dealloc(ptr, layout);
}

// Main execution entry point
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[no_mangle]
pub fn execute(params_offset: i32) {
    let result = match unsafe { handle_execution(params_offset) } {
        Ok(result) => result,
        Err(e) => {
            store_error(&e.to_string());
            return;
        }
    };

    if let Err(e) = store_result(&result) {
        store_error(&e.to_string());
    }
}

unsafe fn handle_execution(params_offset: i32) -> Result<ExecutionResult, ExecutionError> {
    let params = std::slice::from_raw_parts(params_offset as *const u8, 1024);
    let payload: ExecutionPayload = borsh::from_slice(params)
        .map_err(|e| ExecutionError::DeserializationError(e.to_string()))?;

    #[cfg(not(target_arch = "wasm32"))]
    let context = Context::with_actor(WasmlAddress::new([0; 32]));

    #[cfg(not(target_arch = "wasm32"))]
    let wasm_result = execute_in_tee(&context, &payload.input)
        .map_err(|e| ExecutionError::ComputationError(e))?;

    #[cfg(target_arch = "wasm32")]
    let wasm_result = execute_wasm(&payload.input)?;

    let stats = ExecutionStats {
        execution_time: 0,
        memory_used: 0,
    };

    Ok(ExecutionResult {
        result: wasm_result.result.clone(),
        state_hash: compute_measurement(&wasm_result.result),
        stats,
        attestations: vec![],
        timestamp: chrono::Utc::now().to_rfc3339(),
        operation_status: None,
        operation_id: None,
        previous_operation_id: None,
    })
}

fn store_result(result: &ExecutionResult) -> Result<(), ExecutionError> {
    let bytes = borsh::to_vec(result)
        .map_err(|e| ExecutionError::SerializationError(e.to_string()))?;
    
    // Store result in memory
    let ptr = unsafe { alloc(bytes.len() as i32) };
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    Ok(())
}

fn store_error(msg: &str) {
    let ptr = unsafe { alloc(msg.len() as i32) };
    unsafe {
        std::ptr::copy_nonoverlapping(msg.as_ptr(), ptr, msg.len());
    }
}

fn compute_measurement(data: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

#[cfg(not(target_arch = "wasm32"))]
fn init_tee() -> Result<(), String> {
    // Initialize TEE environment
    log::info!("Initializing TEE environment");
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn execute_in_tee(
    _context: &Context,
    payload: &[u8],
) -> Result<WasmExecutionResult, String> {
    // Create execution payload from input
    let execution_payload = ExecutionPayload {
        input: payload.to_vec(),
        params: ExecutionParams {
            id_to: "default".to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        },
    };
    
    // Execute the contract
    let result = execute_contract(&execution_payload)
        .map_err(|e| e.to_string())?;
        
    Ok(WasmExecutionResult {
        result: result.result,
        proof: Some(result.state_hash),
    })
}

#[derive(BorshSerialize, BorshDeserialize)]
struct WasmExecutionResult {
    result: Vec<u8>,
    proof: Option<Vec<u8>>,
}

fn execute_wasm(input: &[u8]) -> Result<WasmExecutionResult, String> {
    // Create execution payload from input
    let payload = ExecutionPayload {
        input: input.to_vec(),
        params: ExecutionParams {
            id_to: "default".to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        },
    };
    
    // Execute the contract using the same logic as execute_in_tee
    let result = execute_contract(&payload)
        .map_err(|e| e.to_string())?;
        
    Ok(WasmExecutionResult {
        result: result.result,
        proof: Some(result.state_hash),
    })
}

#[no_mangle]
pub extern "C" fn execute_external(params: *const u8, params_len: usize) -> *mut u8 {
    let params_slice = unsafe { std::slice::from_raw_parts(params, params_len) };
    match execute_impl(params_slice) {
        Ok(result) => {
            let ptr = Box::into_raw(result.into_boxed_slice()) as *mut u8;
            ptr
        }
        Err(e) => {
            let error_msg = format!("Error: {}", e);
            let error_bytes = error_msg.into_bytes();
            let ptr = Box::into_raw(error_bytes.into_boxed_slice()) as *mut u8;
            ptr
        }
    }
}

#[no_mangle]
pub extern "C" fn free_result(ptr: *mut u8, len: usize) {
    unsafe {
        let _ = Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
    }
}

#[derive(Error, Debug)]
pub enum TeeError {
    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Verification error: {0}")]
    VerificationError(String),
}

fn execute_impl(params: &[u8]) -> Result<Vec<u8>, TeeError> {
    // Deserialize payload
    let payload: ExecutionPayload = borsh::from_slice(params)
        .map_err(|e| TeeError::ExecutionError(format!("Failed to deserialize payload: {}", e)))?;

    // Execute the contract
    let result = execute_contract(&payload)
        .map_err(|e| TeeError::ExecutionError(format!("Failed to execute: {}", e)))?;

    // Verify state if expected hash is provided
    if !payload.params.expected_hash.is_empty() {
        verify_state(&result.state_hash, &payload.params.expected_hash)
            .map_err(|e| TeeError::VerificationError(format!("Proof verification failed: {}", e)))?;
    }

    // Serialize the result
    let bytes = borsh::to_vec(&result)
        .map_err(|e| TeeError::ExecutionError(format!("Failed to serialize result: {}", e)))?;

    Ok(bytes)
}

fn execute_contract(payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
    // Example execution
    let result = payload.input.clone();
    let state_hash = vec![0u8; 32];
    
    let stats = ExecutionStats {
        execution_time: 100,
        memory_used: 1024,
    };

    Ok(ExecutionResult {
        result,
        state_hash,
        stats,
        attestations: vec![],
        timestamp: chrono::Utc::now().to_rfc3339(),
        operation_status: Some("success".to_string()),
        operation_id: Some("123".to_string()),
        previous_operation_id: Some("456".to_string()),
    })
}

fn verify_state(state_hash: &[u8], expected_hash: &[u8]) -> Result<(), TeeError> {
    if state_hash != expected_hash {
        return Err(TeeError::VerificationError(format!(
            "State hash mismatch: expected {:?}, got {:?}",
            expected_hash, state_hash
        )));
    }
    Ok(())
}

fn verify_execution_params(params: &ExecutionParams, wasm_bytes: &[u8]) -> Result<bool, TeeError> {
    // Verify execution parameters
    if !params.expected_hash.is_empty() {
        let measurement = compute_measurement(wasm_bytes);
        if measurement != params.expected_hash {
            return Err(TeeError::VerificationError(
                format!("Hash mismatch: expected {:?}, got {:?}",
                    params.expected_hash, measurement)
            ));
        }
    }
    Ok(true)
}

fn verify_result(result: &ExecutionResult) -> Result<bool, TeeError> {
    // Verify execution result
    let measurement = compute_measurement(&result.result);
    if measurement != result.state_hash {
        return Err(TeeError::VerificationError(
            "Proof mismatch".to_string()
        ));
    }
    Ok(true)
}

#[no_mangle]
pub fn compute(input_ptr: *const u8, input_len: usize) -> i32 {
    let input = unsafe {
        std::slice::from_raw_parts(input_ptr, input_len)
    };

    let result = match execute_wasm(input) {
        Ok(result) => result,
        Err(e) => {
            store_error(&e);
            return -1;
        }
    };

    let bytes = match borsh::to_vec(&result) {
        Ok(bytes) => bytes,
        Err(e) => {
            store_error(&e.to_string());
            return -1;
        }
    };

    let ptr = unsafe { alloc(bytes.len() as i32) };
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ptr as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_execution() {
        let input = b"test input";
        let result = execute_wasm(input).unwrap();
        assert_eq!(result.result, input);
        assert!(result.proof.is_some());
    }

    #[test]
    fn test_result_serialization() {
        let stats = ExecutionStats {
            execution_time: 0,
            memory_used: 0,
        };

        let result = ExecutionResult {
            result: b"output".to_vec(),
            state_hash: compute_measurement(b"output"),
            stats,
            attestations: vec![],
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: None,
            operation_id: None,
            previous_operation_id: None,
        };

        let bytes = borsh::to_vec(&result).unwrap();
        let deserialized: ExecutionResult = borsh::from_slice(&bytes).unwrap();

        assert_eq!(deserialized.result, b"output");
    }

    #[test]
    fn test_execution_params() {
        let wasm_bytes = b"test wasm code".to_vec();
        let measurement = compute_measurement(&wasm_bytes);

        // Test with no hash
        let params = ExecutionParams {
            id_to: "test_contract".to_string(),
            expected_hash: vec![],
            detailed_proof: true,
            function_call: "test".to_string(),
        };

        // This should pass since we have no hash to verify
        assert!(verify_execution_params(&params, &wasm_bytes).is_ok());

        // Test with wrong hash
        let params = ExecutionParams {
            id_to: "test_contract".to_string(),
            expected_hash: vec![0; 32],
            detailed_proof: true,
            function_call: "test".to_string(),
        };

        // This should fail since we have a hash mismatch
        assert!(verify_execution_params(&params, &wasm_bytes).is_err());

        // Test with matching hash
        let params = ExecutionParams {
            id_to: "test_contract".to_string(),
            expected_hash: measurement.clone(),
            detailed_proof: true,
            function_call: "test".to_string(),
        };
        assert!(verify_execution_params(&params, &wasm_bytes).is_ok());
    }

    #[test]
    fn test_verify_result() {
        let output = b"test output".to_vec();
        let state_hash = compute_measurement(&output);

        let stats = ExecutionStats {
            execution_time: 0,
            memory_used: 0,
        };

        let result = ExecutionResult {
            result: output,
            state_hash,
            stats,
            attestations: vec![],
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: None,
            operation_id: None,
            previous_operation_id: None,
        };

        assert!(verify_result(&result).is_ok());
    }

    #[test]
    fn test_compute() {
        let input = b"test input";
        let ptr = compute(input.as_ptr(), input.len());
        assert!(ptr > 0);
    }

    #[test]
    fn test_wasm_execution_matches_simulator() {
        // Test input
        let input = b"test contract input";
        
        // Execute via WASM path
        let wasm_result = execute_wasm(input).expect("WASM execution failed");
        
        // Execute via simulator path (non-WASM)
        let context = Context::with_actor(WasmlAddress::new([0; 32]));
        let sim_result = execute_in_tee(&context, input).expect("Simulator execution failed");
        
        // Results should match
        assert_eq!(wasm_result.result, sim_result.result, "Execution results should match");
        assert_eq!(wasm_result.proof, sim_result.proof, "Execution proofs should match");
        
        // Verify non-empty results
        assert!(!wasm_result.result.is_empty(), "WASM result should not be empty");
        assert!(wasm_result.proof.is_some(), "WASM proof should be present");
        
    }
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
struct ExecutionResult {
    result: Vec<u8>,
    state_hash: Vec<u8>,
    stats: ExecutionStats,
    attestations: Vec<TeeAttestation>,
    timestamp: String,
    operation_status: Option<String>,
    operation_id: Option<String>,
    previous_operation_id: Option<String>,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct TeeAttestation {
    enclave_id: Vec<u8>,
    measurement: Vec<u8>,
    timestamp: u64,
    data: Vec<u8>,
    signature: Vec<u8>,
    region_proof: Option<Vec<u8>>,
    enclave_type: TeeType,
}

#[derive(BorshSerialize, BorshDeserialize)]
enum TeeType {
    SGX,
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
struct ExecutionStats {
    execution_time: u64,
    memory_used: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
struct ExecutionPayload {
    input: Vec<u8>,
    params: ExecutionParams,
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
struct ExecutionParams {
    id_to: String,
    function_call: String,
    detailed_proof: bool,
    expected_hash: Vec<u8>,
}