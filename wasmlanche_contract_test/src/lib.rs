use borsh::{BorshDeserialize, BorshSerialize};

/// Result type for contract operations
#[derive(BorshSerialize, BorshDeserialize)]
pub enum ContractResult {
    /// Success with data
    Success(Vec<u8>),
    /// Error with message
    Error(String),
}

/// Safe parameter reading function that handles both length-prefixed and direct formats
fn read_input_params(data: &[u8]) -> Vec<u8> {
    // Add debug logging
    let data_len = data.len();
    debug_log(&format!("Parameter data length: {}", data_len));
    
    if data_len >= 4 {
        // Try to read length prefix (first 4 bytes as little-endian u32)
        let len_bytes = [data[0], data[1], data[2], data[3]];
        let length = u32::from_le_bytes(len_bytes) as usize;
        
        // Validate length is reasonable (0 < len <= 1024)
        if length > 0 && length <= 1024 && data_len >= length + 4 {
            debug_log(&format!("Reading as length-prefixed format: length={}", length));
            // Length-prefixed format: extract the actual data
            return data[4..4+length].to_vec();
        }
    }
    
    // If we got here, assume direct data format
    debug_log(&format!("Reading as direct data format"));
    data.to_vec()
}

/// A helper function for printing debug information
fn debug_log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        // In WebAssembly, we don't have logging, so this is a no-op
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        // In native code, use standard println
        println!("[DEBUG] {}", message);
    }
}

/// Implementation of add operation - adds all bytes in the input
fn add_bytes(input: &[u8]) -> ContractResult {
    let sum: u32 = input.iter().map(|&byte| byte as u32).sum();
    
    // Return the sum as a 4-byte little-endian value
    ContractResult::Success(sum.to_le_bytes().to_vec())
}

/// The WebAssembly entry points

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    // Allocate memory and return pointer
    let mut buffer = Vec::with_capacity(size as usize);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr as usize as u32
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn dealloc(ptr: u32, size: u32) {
    // Deallocate memory
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, 0, size as usize);
    }
}

/// Contract entry point - reads input parameters and calls add_bytes
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn execute(input_ptr: u32, input_len: u32) -> u64 {
    // Safely read the input parameters
    let input_data = unsafe {
        let slice = std::slice::from_raw_parts(input_ptr as *const u8, input_len as usize);
        slice.to_vec()
    };
    
    // Parse input with safe parameter handling
    let params = read_input_params(&input_data);
    
    // Process the parameters and get result
    let result = add_bytes(&params);
    
    // Serialize the result
    let result_data = borsh::to_vec(&result).unwrap_or_else(|e| {
        borsh::to_vec(&ContractResult::Error(format!("Serialization error: {}", e))).unwrap()
    });
    
    // Allocate memory for the result and copy the data
    let result_len = result_data.len() as u32;
    let result_ptr = alloc(result_len);
    
    unsafe {
        let result_slice = std::slice::from_raw_parts_mut(result_ptr as *mut u8, result_len as usize);
        result_slice.copy_from_slice(&result_data);
    }
    
    // Return pointer and length as a u64 (high 32 bits = ptr, low 32 bits = len)
    ((result_ptr as u64) << 32) | (result_len as u64)
}
