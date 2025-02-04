use tee_interface::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;
use std::alloc::Layout;
use sha2::{Sha256, Digest};
use log;

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
    let result_bytes = result.try_to_vec()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution() {
        // Create test payload
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 2, 3],
            params: ExecutionParams::default(),
        };

        // Serialize payload
        let payload_bytes = payload.try_to_vec().unwrap();
        let len = payload_bytes.len() as u32;
        let len_bytes = len.to_le_bytes();

        // Create params buffer
        let mut params = vec![0u8; 8 + len as usize];
        params[4..8].copy_from_slice(&len_bytes);
        params[8..].copy_from_slice(&payload_bytes);

        let params_ptr = params.as_ptr() as i32;

        execute(params_ptr);
    }

    #[test]
    fn test_measurement() {
        let data = vec![1, 2, 3, 4];
        let measurement = compute_measurement(&data);
        assert_eq!(measurement.len(), 32);
    }
}