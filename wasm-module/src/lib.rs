use tee_interface::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;
use std::alloc::Layout;
use sha2::{Sha256, Digest};
use log;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;

mod computation;
pub use computation::*;

const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10MB
const MIN_PAYLOAD_SIZE: usize = 8; // At least length prefix
const MAX_RESULT_SIZE: usize = 10 * 1024 * 1024; // 10MB

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

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Memory error: {0}")]
    MemoryError(String),

    #[error("System error: {0}")]
    SystemError(String),
}

type Result<T> = std::result::Result<T, TeeError>;

// Convert ExecutionError to TeeError
impl From<ExecutionError> for TeeError {
    fn from(err: ExecutionError) -> Self {
        match err {
            ExecutionError::DeserializationError(msg) => TeeError::ExecutionError(format!("Deserialization error: {}", msg)),
            ExecutionError::ComputationError(msg) => TeeError::ExecutionError(format!("Computation error: {}", msg)),
            ExecutionError::SerializationError(msg) => TeeError::ExecutionError(format!("Serialization error: {}", msg)),
            ExecutionError::StorageError(msg) => TeeError::ExecutionError(format!("Storage error: {}", msg)),
            ExecutionError::InvalidInput(msg) => TeeError::ExecutionError(format!("Invalid input: {}", msg)),
            ExecutionError::MemoryError(msg) => TeeError::ExecutionError(format!("Memory error: {}", msg)),
            ExecutionError::SystemError(msg) => TeeError::ExecutionError(format!("System error: {}", msg)),
        }
    }
}

// Required memory export for Hypersdk
#[no_mangle]
static mut MEMORY: [u8; 0] = [];

// Thread-safe result storage
struct ResultStorage {
    buffer: UnsafeCell<Vec<u8>>,
    size: AtomicUsize,
}

unsafe impl Sync for ResultStorage {}

static RESULT_STORAGE: ResultStorage = ResultStorage {
    buffer: UnsafeCell::new(Vec::new()),
    size: AtomicUsize::new(0),
};

// Required memory allocation functions
#[no_mangle]
pub unsafe fn alloc(size: i32) -> *mut u8 {
    if size <= 0 {
        return std::ptr::null_mut();
    }
    
    match Layout::from_size_align(size as usize, 8) {
        Ok(layout) => std::alloc::alloc(layout),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe fn dealloc(ptr: *mut u8, size: i32) {
    if ptr.is_null() || size <= 0 {
        return;
    }
    
    if let Ok(layout) = Layout::from_size_align(size as usize, 8) {
        std::alloc::dealloc(ptr, layout);
    }
}

#[no_mangle]
pub unsafe fn execute(params_offset: i32) {
    match handle_execution(params_offset) {
        Ok(result) => {
            if let Err(e) = store_result(&result) {
                store_error(&format!("Failed to store result: {}", e));
            }
        }
        Err(e) => store_error(&format!("Execution failed: {}", e)),
    }
}

unsafe fn handle_execution(params_offset: i32) -> Result<tee_interface::ExecutionResult> {
    // Validate params_offset
    if params_offset < 0 {
        return Err(ExecutionError::InvalidInput("Invalid params offset".into()).into());
    }

    // Read payload length (4 bytes prefix)
    let len_bytes = core::slice::from_raw_parts(
        (params_offset as *const u8).offset(4),
        4
    );
    
    let len = u32::from_le_bytes(len_bytes.try_into()
        .map_err(|_| ExecutionError::MemoryError("Failed to read length bytes".into()))?) as usize;

    // Validate payload size
    if len < MIN_PAYLOAD_SIZE {
        return Err(ExecutionError::InvalidInput(format!(
            "Payload size too small: {} bytes", len
        )).into());
    }

    if len > MAX_PAYLOAD_SIZE {
        return Err(ExecutionError::InvalidInput(format!(
            "Payload size too large: {} bytes", len
        )).into());
    }

    // Read payload
    let payload_bytes = core::slice::from_raw_parts(
        (params_offset as *const u8).offset(8),
        len
    );

    // Validate payload alignment
    if (payload_bytes.as_ptr() as usize) % 8 != 0 {
        return Err(ExecutionError::MemoryError("Payload not 8-byte aligned".into()).into());
    }

    // Deserialize payload
    let payload: ExecutionPayload = BorshDeserialize::try_from_slice(payload_bytes)
        .map_err(|e| ExecutionError::DeserializationError(format!("Failed to deserialize payload: {}", e)))?;

    // Validate payload fields
    if payload.input.is_empty() {
        return Err(ExecutionError::InvalidInput("Empty input data".into()).into());
    }

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
        .map_err(|e| ExecutionError::SystemError(format!("Failed to get timestamp: {}", e)))?
        .as_secs();

    // Create SGX attestation
    let sgx_attestation = TeeAttestation {
        tee_type: TeeType::Sgx,
        measurement: PlatformMeasurement::Sgx {
            mrenclave: compute_mrenclave(&result),
            mrsigner: compute_mrsigner(),
            miscselect: 0,
            attributes: [0u8; 16],
        },
        signature: vec![],
    };

    // Create SEV attestation
    let sev_attestation = TeeAttestation {
        tee_type: TeeType::Sev,
        measurement: PlatformMeasurement::Sev {
            measurement: compute_measurement(&result),
            platform_info: [0u8; 32],
            launch_digest: [0u8; 32],
        },
        signature: vec![],
    };

    Ok(tee_interface::ExecutionResult {
        tx_id: vec![],
        state_hash,
        output: result,
        attestations: [sgx_attestation, sev_attestation],
        timestamp,
        region_id: String::from("default"),
    })
}

fn store_result(result: &tee_interface::ExecutionResult) -> Result<()> {
    let result_bytes = result.try_to_vec()
        .map_err(|e| ExecutionError::SerializationError(format!("Failed to serialize result: {}", e)))?;

    if result_bytes.len() > MAX_RESULT_SIZE {
        return Err(ExecutionError::StorageError(format!(
            "Result size {} exceeds maximum allowed size {}", 
            result_bytes.len(), 
            MAX_RESULT_SIZE
        )).into());
    }

    // Store result in shared memory buffer
    unsafe {
        *RESULT_STORAGE.buffer.get() = result_bytes;
        RESULT_STORAGE.size.store((*RESULT_STORAGE.buffer.get()).len(), Ordering::SeqCst);
    }

    Ok(())
}

// Helper function to retrieve stored result
#[no_mangle]
pub unsafe fn get_result_ptr() -> *const u8 {
    (*RESULT_STORAGE.buffer.get()).as_ptr()
}

#[no_mangle]
pub unsafe fn get_result_len() -> usize {
    RESULT_STORAGE.size.load(Ordering::SeqCst)
}

fn compute_mrenclave(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn compute_mrsigner() -> [u8; 32] {
    [0u8; 32] // TODO: Implement real mrsigner calculation
}

fn compute_measurement(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn store_error(msg: &str) {
    log::error!("Execution error: {}", msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution() {
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 2, 3],
            params: ExecutionParams::default(),
        };

        let payload_bytes = payload.try_to_vec().unwrap();
        let mut params = vec![0u8; 4]; // padding
        params.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
        params.extend_from_slice(&payload_bytes);

        let params_ptr = params.as_ptr();
        let result = unsafe { handle_execution(params_ptr as i32) }.unwrap();

        assert!(!result.output.is_empty());
        assert_eq!(result.attestations.len(), 2);
        assert_eq!(result.attestations[0].tee_type, TeeType::Sgx);
        assert_eq!(result.attestations[1].tee_type, TeeType::Sev);
    }

    #[test]
    fn test_measurement() {
        let data = b"test data";
        let measurement = compute_measurement(data);
        assert_eq!(measurement.len(), 32);
    }

    #[test]
    fn test_result_storage() {
        let result = tee_interface::ExecutionResult {
            tx_id: vec![1, 2, 3],
            state_hash: [0u8; 32],
            output: vec![4, 5, 6],
            attestations: [
                TeeAttestation {
                    tee_type: TeeType::Sgx,
                    measurement: PlatformMeasurement::Sgx {
                        mrenclave: [0u8; 32],
                        mrsigner: [0u8; 32],
                        miscselect: 0,
                        attributes: [0u8; 16],
                    },
                    signature: vec![],
                },
                TeeAttestation {
                    tee_type: TeeType::Sev,
                    measurement: PlatformMeasurement::Sev {
                        measurement: [0u8; 32],
                        platform_info: [0u8; 32],
                        launch_digest: [0u8; 32],
                    },
                    signature: vec![],
                },
            ],
            timestamp: 12345,
            region_id: "test-region".to_string(),
        };

        // Store result
        store_result(&result).unwrap();

        // Get stored result
        unsafe {
            let ptr = get_result_ptr();
            let len = get_result_len();
            
            // Read stored bytes
            let stored_bytes = std::slice::from_raw_parts(ptr, len);
            
            // Deserialize and verify
            let stored_result: tee_interface::ExecutionResult = BorshDeserialize::try_from_slice(stored_bytes).unwrap();
            
            assert_eq!(stored_result.tx_id, result.tx_id);
            assert_eq!(stored_result.state_hash, result.state_hash);
            assert_eq!(stored_result.output, result.output);
            assert_eq!(stored_result.timestamp, result.timestamp);
            assert_eq!(stored_result.region_id, result.region_id);
        }
    }

    #[test]
    fn test_result_storage_size_limit() {
        let mut large_result = tee_interface::ExecutionResult {
            tx_id: vec![],
            state_hash: [0u8; 32],
            output: vec![0; MAX_RESULT_SIZE + 1],
            attestations: [
                TeeAttestation {
                    tee_type: TeeType::Sgx,
                    measurement: PlatformMeasurement::Sgx {
                        mrenclave: [0u8; 32],
                        mrsigner: [0u8; 32],
                        miscselect: 0,
                        attributes: [0u8; 16],
                    },
                    signature: vec![],
                },
                TeeAttestation {
                    tee_type: TeeType::Sev,
                    measurement: PlatformMeasurement::Sev {
                        measurement: [0u8; 32],
                        platform_info: [0u8; 32],
                        launch_digest: [0u8; 32],
                    },
                    signature: vec![],
                },
            ],
            timestamp: 12345,
            region_id: "test-region".to_string(),
        };

        // Attempt to store oversized result
        let result = store_result(&large_result);
        assert!(result.is_err());
        
        if let Err(e) = result {
            match e {
                TeeError::ExecutionError(msg) => {
                    assert!(msg.contains("Result size"));
                    assert!(msg.contains("exceeds maximum allowed size"));
                }
                _ => panic!("Expected ExecutionError"),
            }
        }

        // Verify small result still works
        large_result.output = vec![0; 1000]; // Much smaller output
        assert!(store_result(&large_result).is_ok());
    }
}