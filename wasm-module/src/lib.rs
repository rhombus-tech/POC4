use wasmlanche::Context;
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
        return;
    }
}

unsafe fn handle_execution(params_offset: i32) -> Result<ExecutionResult, ExecutionError> {
    // Read payload length (4 bytes prefix)
    let len_bytes = core::slice::from_raw_parts(
        (params_offset as *const u8).offset(4),
        4
    );
    let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;

    // Read payload
    let payload_bytes = core::slice::from_raw_parts(
        (params_offset as *const u8).offset(8),
        len
    );

    // Deserialize payload
    let payload: ExecutionPayload = BorshDeserialize::try_from_slice(payload_bytes)
        .map_err(|e| ExecutionError::DeserializationError(format!("Failed to deserialize payload: {}", e)))?;

    // Initialize computation engine
    let mut engine = ComputationEngine::new();

    // Execute computation
    let result = engine.execute(payload)
        .map_err(|e| ExecutionError::ComputationError(format!("Computation failed: {}", e)))?;

    // Calculate result hash
    let mut hasher = Sha256::new();
    hasher.update(&result);
    let state_hash = hasher.finalize().into();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let attestation = TeeAttestation {
        tee_type: TeeType::Sgx,
        measurement: compute_measurement(&result),
        signature: vec![],
    };

    Ok(ExecutionResult {
        tx_id: vec![],
        state_hash,
        output: result,
        attestations: vec![attestation],
        timestamp,
        region_id: String::from("default"),
    })
}

fn store_result(result: &ExecutionResult) -> Result<(), ExecutionError> {
    let result_bytes = borsh::to_vec(result)
        .map_err(|e| ExecutionError::SerializationError(format!("Failed to serialize result: {}", e)))?;
    
    // TODO: Store result in shared memory
    // For now, we just log the size
    log::debug!("Storing result of size {}", result_bytes.len());
    Ok(())
}

fn store_error(msg: &str) {
    // TODO: Store error in shared memory
    // For now, we just log the error
    log::error!("Execution error: {}", msg);
}

fn compute_measurement(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Initialize the TEE environment
pub fn init_tee() -> Result<(), String> {
    // In a real implementation, we would:
    // 1. Initialize TEE runtime
    // 2. Set up secure communication channels
    // 3. Load necessary keys and certificates
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

    // In a real implementation, we would:
    // 1. Verify payload signature
    // 2. Load code into TEE
    // 3. Execute code
    // 4. Generate attestation
    // 5. Return result with attestation

    let attestation = TeeAttestation {
        tee_type: TeeType::Sgx,
        measurement: vec![1u8; 32],
        signature: vec![],
    };

    let result = ExecutionResult {
        tx_id: vec![1],
        output: b"test_output".to_vec(),
        state_hash: [0u8; 32],
        attestations: vec![attestation],
        timestamp: context.timestamp(),
        region_id: "0".to_string(),
    };

    Ok(result)
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct WasmExecutionResult {
    pub output: Vec<u8>,
    pub state: Vec<u8>,
}

pub fn execute_wasm(input: &[u8]) -> Result<WasmExecutionResult, String> {
    // In a real implementation, we would:
    // 1. Initialize WASM runtime
    // 2. Load and validate WASM module
    // 3. Execute WASM code with input
    // 4. Collect output and state
    
    Ok(WasmExecutionResult {
        output: input.to_vec(),
        state: vec![],
    })
}

pub fn verify_execution_params(params: &ExecutionParams, wasm_bytes: &[u8]) -> Result<bool, TeeError> {
    if let Some(expected_hash) = params.expected_hash {
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        let actual_hash: [u8; 32] = hasher.finalize().into();

        if actual_hash != expected_hash {
            return Err(TeeError::ExecutionError("WASM hash mismatch".to_string()));
        }
    }

    Ok(true)
}

pub fn verify_result(_result: &ExecutionResult) -> Result<bool, TeeError> {
    // In a real implementation, we would:
    // 1. Verify attestations
    // 2. Verify state hash
    // 3. Verify output integrity
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_execution() {
        let input = b"test input";
        let result = execute_wasm(input).unwrap();
        assert_eq!(result.output, input);
    }

    #[test]
    fn test_result_serialization() {
        let result = WasmExecutionResult {
            output: b"test output".to_vec(),
            state: b"test state".to_vec(),
        };

        let bytes = borsh::to_vec(&result).unwrap();
        let decoded: WasmExecutionResult = borsh::from_slice(&bytes).unwrap();

        assert_eq!(decoded.output, result.output);
        assert_eq!(decoded.state, result.state);
    }

    #[test]
    fn test_execution_params() {
        let wasm_bytes = b"\0asm\x01\0\0\0";
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        let hash: [u8; 32] = hasher.finalize().into();

        let params = ExecutionParams {
            expected_hash: Some(hash),
            detailed_proof: true,
        };

        assert!(verify_execution_params(&params, wasm_bytes).unwrap());
    }

    #[test]
    fn test_verify_result() {
        let result = ExecutionResult {
            tx_id: vec![1],
            output: vec![2],
            state_hash: [0; 32],
            attestations: vec![],
            timestamp: 123456789,
            region_id: "test".to_string(),
        };

        assert!(verify_result(&result).unwrap());
    }
}