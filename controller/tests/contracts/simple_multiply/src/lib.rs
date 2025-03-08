#![no_std]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

#[cfg(target_arch = "wasm32")]
use alloc::vec::Vec;
#[cfg(target_arch = "wasm32")]
use alloc::string::{String, ToString};

// Import what we need from wasmlanche
use wasmlanche::Context;

// Define the external imports required by the WASM environment
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn set_call_result(ptr: *const u8, len: usize);
}

// Simple hard-coded storage key prefix
const RESULT_PREFIX: &[u8] = b"multiply_result_";

/// Check if there's a cached result 
fn get_cached_result(context: &mut Context, a: u64, b: u64) -> Option<u64> {
    // Create a key combining the prefix and the operands
    let mut key = Vec::with_capacity(RESULT_PREFIX.len() + 16); // 8 bytes each for a and b
    key.extend_from_slice(RESULT_PREFIX);
    key.extend_from_slice(&a.to_le_bytes());
    key.extend_from_slice(&b.to_le_bytes());
    
    // Try to get data directly using low-level API
    match context.get_by_key(&key) {
        Ok(Some(value)) => {
            if value.len() >= 8 {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&value[0..8]);
                Some(u64::from_le_bytes(bytes))
            } else {
                None
            }
        },
        _ => None,
    }
}

/// Store a result in the cache
fn store_result(context: &mut Context, a: u64, b: u64, result: u64) {
    // Create a key combining the prefix and the operands
    let mut key = Vec::with_capacity(RESULT_PREFIX.len() + 16); // 8 bytes each for a and b
    key.extend_from_slice(RESULT_PREFIX);
    key.extend_from_slice(&a.to_le_bytes());
    key.extend_from_slice(&b.to_le_bytes());
    
    // Store the result as bytes
    let value = result.to_le_bytes().to_vec();
    
    // Store the data directly
    let _ = context.store_by_key(&key, value);
}

/// Multiply two numbers and cache the result
fn multiply_impl(context: &mut Context, a: u64, b: u64) -> u64 {
    // Check if we already have this result
    if let Some(result) = get_cached_result(context, a, b) {
        return result;
    }
    
    // Compute the result
    let result = a.checked_mul(b).unwrap_or(0);
    
    // Store the result
    store_result(context, a, b, result);
    
    result
}

// Direct export function
#[no_mangle]
pub extern "C" fn multiply_direct(a: u64, b: u64) -> u64 {
    let mut ctx = Context::new();
    multiply_impl(&mut ctx, a, b)
}

// Standard execute entry point for the TEE controller
#[no_mangle]
pub extern "C" fn execute(input_ptr: *const u8, input_size: usize) {
    #[cfg(target_arch = "wasm32")]
    {
        // Create a context
        let mut ctx = Context::new();
        
        // Read the input data safely
        let input_data = unsafe {
            let slice = core::slice::from_raw_parts(input_ptr, input_size);
            slice
        };
        
        // Convert the input to a string
        let input_str = core::str::from_utf8(input_data).unwrap_or_default();
        
        // Parse the command and arguments
        let parts: Vec<&str> = input_str.split(',').collect();
        
        if parts.len() >= 3 {
            let command = parts[0];
            
            // Handle different commands
            if command == "multiply" {
                // Parse the numbers
                if let (Ok(a), Ok(b)) = (parts[1].parse::<u64>(), parts[2].parse::<u64>()) {
                    // Execute the multiplication
                    let result = multiply_impl(&mut ctx, a, b);
                    
                    // Convert the result to a string
                    let result_str = result.to_string();
                    let result_bytes = result_str.as_bytes();
                    
                    // Set the result
                    unsafe {
                        set_call_result(result_bytes.as_ptr(), result_bytes.len());
                    }
                    
                    return;
                }
            }
        }
        
        // If we get here, there was an error
        let error_msg = "Invalid command or arguments";
        unsafe {
            set_call_result(error_msg.as_ptr(), error_msg.len());
        }
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Non-WASM implementation (for testing)
        let _ = (input_ptr, input_size);
    }
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
