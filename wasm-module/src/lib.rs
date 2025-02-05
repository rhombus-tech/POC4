#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use wasmlanche::{Context, Address};

use tee_interface::prelude::*;
use thiserror::Error;
use sha2::{Sha256, Digest};
use std::alloc::Layout;
use log;
use borsh::{BorshSerialize, BorshDeserialize};

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
    let context = Context::with_actor(Address::default());

    #[cfg(not(target_arch = "wasm32"))]
    let wasm_result = execute_in_tee(&context, &payload.input)
        .map_err(|e| ExecutionError::ComputationError(e))?;

    #[cfg(target_arch = "wasm32")]
    let wasm_result = execute_wasm(&payload.input)?;

    let stats = ExecutionStats {
        execution_time: 0,
        memory_used: 0,
        syscall_count: 0,
    };

    let attestation = TeeAttestation {
        enclave_id: [0u8; 32],
        measurement: compute_measurement(&payload.input),
        data: b"WASM execution".to_vec(),
        signature: vec![0u8; 64],
        region_proof: None,
    };

    Ok(ExecutionResult {
        result: wasm_result.output,
        attestation,
        state_hash: wasm_result.proof.unwrap_or_default(),
        stats,
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
    // Execute code in TEE
    log::info!("Executing in TEE");
    execute_wasm(payload)
}

#[derive(BorshSerialize, BorshDeserialize)]
struct WasmExecutionResult {
    output: Vec<u8>,
    proof: Option<Vec<u8>>,
}

fn execute_wasm(input: &[u8]) -> Result<WasmExecutionResult, String> {
    let measurement = compute_measurement(input);
    
    Ok(WasmExecutionResult {
        output: input.to_vec(),
        proof: Some(measurement),
    })
}

fn verify_execution_params(params: &ExecutionParams, wasm_bytes: &[u8]) -> Result<bool, TeeError> {
    // Verify execution parameters
    if let Some(expected_hash) = params.expected_hash {
        let measurement = compute_measurement(wasm_bytes);
        if measurement != expected_hash.to_vec() {
            return Err(TeeError::VerificationError(
                "Hash mismatch".to_string()
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
        assert_eq!(result.output, input);
        assert!(result.proof.is_some());
    }

    #[test]
    fn test_result_serialization() {
        let stats = ExecutionStats {
            execution_time: 0,
            memory_used: 0,
            syscall_count: 0,
        };

        let attestation = TeeAttestation {
            enclave_id: [0u8; 32],
            measurement: vec![2u8; 32],
            data: b"test".to_vec(),
            signature: vec![3u8; 64],
            region_proof: None,
        };

        let result = ExecutionResult {
            result: b"output".to_vec(),
            attestation,
            state_hash: vec![5u8; 32],
            stats,
        };

        let bytes = borsh::to_vec(&result).unwrap();
        let deserialized: ExecutionResult = borsh::from_slice(&bytes).unwrap();

        assert_eq!(deserialized.result, b"output");
        assert_eq!(deserialized.state_hash, vec![5u8; 32]);
    }

    #[test]
    fn test_execution_params() {
        let wasm_bytes = b"test wasm";
        let measurement = compute_measurement(wasm_bytes);

        let params = ExecutionParams {
            expected_hash: Some([0u8; 32]),
            detailed_proof: true,
        };

        // This should fail since we have a hash mismatch
        assert!(verify_execution_params(&params, wasm_bytes).is_err());

        // Test with matching hash
        let params = ExecutionParams {
            expected_hash: Some(measurement[0..32].try_into().unwrap()),
            detailed_proof: true,
        };
        assert!(verify_execution_params(&params, wasm_bytes).is_ok());
    }

    #[test]
    fn test_verify_result() {
        let output = b"test output".to_vec();
        let state_hash = compute_measurement(&output);

        let stats = ExecutionStats {
            execution_time: 0,
            memory_used: 0,
            syscall_count: 0,
        };

        let attestation = TeeAttestation {
            enclave_id: [0u8; 32],
            measurement: state_hash.clone(),
            data: b"test".to_vec(),
            signature: vec![3u8; 64],
            region_proof: None,
        };

        let result = ExecutionResult {
            result: output,
            attestation,
            state_hash,
            stats,
        };

        assert!(verify_result(&result).is_ok());
    }

    #[test]
    fn test_compute() {
        let input = b"test input";
        let ptr = compute(input.as_ptr(), input.len());
        assert!(ptr > 0);
    }
}