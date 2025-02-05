use wasmlanche::Context;
use tee_interface::prelude::*;
use thiserror::Error;
use sha2::{Sha256, Digest};
use std::alloc::Layout;
use log;
use borsh::{BorshSerialize, BorshDeserialize};
use wasmlanche::Address;

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
    let payload: ExecutionPayload = BorshDeserialize::deserialize(&mut &params[..])
        .map_err(|e| ExecutionError::DeserializationError(e.to_string()))?;

    // Initialize TEE environment
    init_tee().map_err(|e| ExecutionError::ComputationError(e))?;

    // Create execution context with default actor
    let context = Context::with_actor(Address::default());

    // Execute in TEE
    let result = execute_in_tee(&context, &params)
        .map_err(|e| ExecutionError::ComputationError(e))?;

    Ok(result)
}

fn store_result(result: &ExecutionResult) -> Result<(), ExecutionError> {
    let bytes = borsh::to_vec(result)
        .map_err(|e| ExecutionError::SerializationError(e.to_string()))?;

    // Store result in memory for retrieval
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), 0 as *mut u8, bytes.len());
    }
    Ok(())
}

fn store_error(msg: &str) {
    unsafe {
        std::ptr::copy_nonoverlapping(msg.as_ptr(), 0 as *mut u8, msg.len());
    }
}

fn compute_measurement(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Initialize the TEE environment
fn init_tee() -> Result<(), String> {
    // In a real implementation, we would:
    // 1. Initialize TEE environment
    // 2. Set up memory protection
    // 3. Load keys and certificates
    Ok(())
}

/// Execute code in TEE
pub fn execute_in_tee(
    context: &Context,
    payload: &[u8],
) -> Result<ExecutionResult, String> {
    // Deserialize payload
    let payload: ExecutionPayload = BorshDeserialize::deserialize(&mut &payload[..])
        .map_err(|e| format!("Failed to deserialize payload: {}", e))?;

    // Execute WASM code
    let wasm_result = execute_wasm(&payload.input)?;

    // Generate attestation
    let attestation = TeeAttestation {
        enclave_id: [1u8; 32], // TODO: Get real enclave ID
        measurement: compute_measurement(&payload.input),
        data: b"Execution completed".to_vec(),
        signature: vec![1u8; 64], // TODO: Generate real signature
        region_proof: Some(vec![1u8; 32]), // TODO: Get real proof
    };

    // Track execution stats
    let stats = ExecutionStats {
        execution_time: context.timestamp() as u64,
        memory_used: wasm_result.state.len() as u64,
        syscall_count: 10, // TODO: Track real syscalls
    };

    // Calculate state hash
    let mut hasher = Sha256::new();
    hasher.update(&wasm_result.state);
    let state_hash = hasher.finalize().to_vec();

    let result = ExecutionResult {
        result: wasm_result.output,
        attestation,
        state_hash,
        stats,
    };

    Ok(result)
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct WasmExecutionResult {
    pub output: Vec<u8>,
    pub state: Vec<u8>,
}

fn execute_wasm(input: &[u8]) -> Result<WasmExecutionResult, String> {
    // In a real implementation, we would:
    // 1. Load WASM module
    // 2. Set up execution environment
    // 3. Execute code
    // 4. Collect results
    Ok(WasmExecutionResult {
        output: input.to_vec(),
        state: vec![0u8; 32],
    })
}

fn verify_execution_params(params: &ExecutionParams, wasm_bytes: &[u8]) -> Result<bool, TeeError> {
    // Verify input size
    if wasm_bytes.len() > constants::MAX_INPUT_SIZE {
        return Err(TeeError::ExecutionError("Input too large".into()));
    }

    // Verify hash if provided
    if let Some(expected_hash) = params.expected_hash {
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        let actual_hash = hasher.finalize();
        if actual_hash.as_slice() != expected_hash {
            return Err(TeeError::ExecutionError("Hash mismatch".into()));
        }
    }

    Ok(true)
}

fn verify_result(result: &ExecutionResult) -> Result<bool, TeeError> {
    // Verify attestation
    if result.attestation.measurement.is_empty() {
        return Err(TeeError::ExecutionError("Missing measurement".into()));
    }

    // Verify state hash
    if result.state_hash.is_empty() {
        return Err(TeeError::ExecutionError("Missing state hash".into()));
    }

    Ok(true)
}

#[no_mangle]
pub extern "C" fn compute(input_ptr: *const u8, input_len: usize) -> i32 {
    // Convert input to slice
    let input = unsafe {
        std::slice::from_raw_parts(input_ptr, input_len)
    };

    // Create engine and execute
    let mut engine = ComputationEngine::new();
    let payload = ExecutionPayload {
        input: input.to_vec(),
        ..Default::default()
    };

    // Execute and return result
    match engine.execute(payload) {
        Ok(result) => {
            if result.len() != 1 {
                return -1;
            }
            result[0] as i32
        }
        Err(_) => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_execution() {
        let input = b"test input";
        let result = execute_wasm(input).unwrap();
        assert_eq!(result.output, input);
        assert_eq!(result.state.len(), 32);
    }

    #[test]
    fn test_result_serialization() {
        let attestation = TeeAttestation {
            enclave_id: [1u8; 32],
            measurement: vec![2u8; 32],
            data: b"test".to_vec(),
            signature: vec![3u8; 64],
            region_proof: Some(vec![4u8; 32]),
        };

        let result = ExecutionResult {
            result: b"output".to_vec(),
            attestation,
            state_hash: vec![5u8; 32],
            stats: ExecutionStats {
                execution_time: 1000,
                memory_used: 1024,
                syscall_count: 10,
            },
        };

        let bytes = borsh::to_vec(&result).unwrap();
        let deserialized: ExecutionResult = BorshDeserialize::deserialize(&mut &bytes[..]).unwrap();
        assert_eq!(deserialized.result, b"output");
    }

    #[test]
    fn test_execution_params() {
        let params = ExecutionParams {
            expected_hash: Some([0u8; 32]),
            detailed_proof: true,
        };

        let wasm_bytes = vec![0u8; 1024];
        assert!(verify_execution_params(&params, &wasm_bytes).is_ok());

        let large_wasm = vec![0u8; constants::MAX_INPUT_SIZE + 1];
        assert!(verify_execution_params(&params, &large_wasm).is_err());
    }

    #[test]
    fn test_verify_result() {
        let attestation = TeeAttestation {
            enclave_id: [1u8; 32],
            measurement: vec![2u8; 32],
            data: b"test".to_vec(),
            signature: vec![3u8; 64],
            region_proof: Some(vec![4u8; 32]),
        };

        let result = ExecutionResult {
            result: b"output".to_vec(),
            attestation,
            state_hash: vec![5u8; 32],
            stats: ExecutionStats {
                execution_time: 1000,
                memory_used: 1024,
                syscall_count: 10,
            },
        };

        assert!(verify_result(&result).is_ok());
    }

    #[test]
    fn test_compute() {
        let input = vec![1, 2, 3, 4];
        let result = compute(input.as_ptr(), input.len());
        assert_eq!(result, 10); // 1 + 2 + 3 + 4 = 10
    }
}