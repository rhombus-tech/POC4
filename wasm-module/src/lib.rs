use borsh::{BorshDeserialize, BorshSerialize};
use tee_interface::prelude::*;
use sallyport::guest::{self, Platform};
use core::alloc::Layout;
use sha2::{Sha256, Digest};

mod computation;
use computation::ComputationEngine;

// Required memory export for Hypersdk
#[no_mangle]
static mut MEMORY: [u8; 0] = [];

// Memory allocation function required by Hypersdk
#[no_mangle]
pub extern "C" fn alloc(size: i32) -> i32 {
    if size <= 0 {
        return 0; 
    }
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    let ptr = unsafe { std::alloc::alloc(layout) };
    ptr as i32
}

// Import set_call_result from Hypersdk
#[link(wasm_import_module = "contract")]
extern "C" {
    fn set_call_result(ptr: i32, len: i32);
}

// Get pointer to start of linear memory
fn get_memory_ptr() -> *mut u8 {
    extern "C" {
        static mut MEMORY: u8;
    }
    unsafe { &mut MEMORY as *mut u8 }
}

// Main TEE execution entrypoint for Hypersdk
#[no_mangle]
pub extern "C" fn execute(params_offset: i32) {
    let result = match unsafe { handle_execution(params_offset) } {
        Ok(result) => result,
        Err(err) => {
            store_error(&err);
            return;
        }
    };

    // Store successful result
    match borsh::to_vec(&result) {
        Ok(bytes) => store_result(&bytes),
        Err(e) => store_error(&format!("Failed to serialize result: {}", e)),
    }
}

unsafe fn handle_execution(params_offset: i32) -> Result<ExecutionResult, String> {
    // Read payload length (4 bytes prefix)
    let offset = params_offset as usize;
    let len_bytes = core::slice::from_raw_parts(
        get_memory_ptr().add(offset),
        4
    );
    let payload_len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;

    // Read full payload
    let payload_bytes = core::slice::from_raw_parts(
        get_memory_ptr().add(offset + 4),
        payload_len
    );

    // Deserialize payload
    let payload: ExecutionPayload = borsh::from_slice(payload_bytes)
        .map_err(|e| format!("Failed to deserialize payload: {}", e))?;

    // Initialize platform and computation engine
    let platform = Platform::default();
    let mut engine = ComputationEngine::new();

    // Execute computation
    let result = engine.execute(payload)
        .map_err(|e| format!("Computation failed: {}", e))?;

    // Get attestation
    let attestation = get_attestation(&platform)?;

    // Calculate result hash
    let mut hasher = Sha256::new();
    hasher.update(&result);
    let result_hash = hasher.finalize().into();

    Ok(ExecutionResult {
        result_hash,
        result,
        attestation,
    })
}

fn get_attestation(platform: &Platform) -> Result<AttestationReport, String> {
    let measurement = platform.get_measurement()
        .map_err(|e| format!("Failed to get measurement: {}", e))?;

    let timestamp = platform.get_time()
        .map_err(|e| format!("Failed to get timestamp: {}", e))?;

    Ok(AttestationReport {
        enclave_type: match platform.platform_type() {
            guest::PlatformType::Sgx => EnclaveType::IntelSGX,
            guest::PlatformType::Sev => EnclaveType::AMDSEV,
            _ => return Err("Unsupported platform type".into()),
        },
        measurement,
        timestamp,
        platform_data: Vec::new(), // Add platform-specific data as needed
    })
}

fn store_result(data: &[u8]) {
    let result_len = data.len();
    let result_offset = alloc(result_len as i32);
    
    unsafe {
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            get_memory_ptr().add(result_offset as usize),
            result_len
        );
        set_call_result(result_offset, result_len as i32);
    }
}

fn store_error(error: &str) {
    let error_bytes = error.as_bytes();
    let error_len = error_bytes.len();
    let error_offset = alloc(error_len as i32);
    
    unsafe {
        core::ptr::copy_nonoverlapping(
            error_bytes.as_ptr(),
            get_memory_ptr().add(error_offset as usize),
            error_len
        );
        set_call_result(error_offset, -(error_len as i32)); // Negative length indicates error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc() {
        let size = 100;
        let ptr = alloc(size);
        assert!(ptr > 0);
    }

    #[test]
    fn test_execution() {
        // Create test payload
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 2, 3],
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };
        
        // Serialize and write to memory
        let payload_bytes = borsh::to_vec(&payload).unwrap();
        let payload_len = payload_bytes.len();
        
        // Allocate memory for length prefix + payload
        let total_size = 4 + payload_len;
        let mem_offset = alloc(total_size as i32);
        
        unsafe {
            // Write length prefix
            core::ptr::copy_nonoverlapping(
                (payload_len as u32).to_le_bytes().as_ptr(),
                get_memory_ptr().add(mem_offset as usize),
                4
            );
            // Write payload
            core::ptr::copy_nonoverlapping(
                payload_bytes.as_ptr(),
                get_memory_ptr().add(mem_offset as usize + 4),
                payload_len
            );
        }
        
        // Execute (note: this test just verifies it doesn't crash)
        execute(mem_offset);
    }
}