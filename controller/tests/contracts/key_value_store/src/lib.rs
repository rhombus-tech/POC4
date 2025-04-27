#![no_std]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

#[cfg(target_arch = "wasm32")]
use alloc::{string::{String, ToString}, vec::Vec, boxed::Box};

// Import what we need from wasmlanche
use wasmlanche::Context;

// Define the external imports required by the WASM environment
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn set_call_result(ptr: *const u8, len: usize);
}

/// Store a key-value pair
fn store_impl(context: &mut Context, key: &str, value: &[u8]) -> bool {
    // Just use the key string bytes directly
    let key_bytes = key.as_bytes();
    
    // Store using low-level API
    match context.store_by_key(key_bytes, value.to_vec()) {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Get a value by key
fn get_impl(context: &mut Context, key: &str) -> Option<Vec<u8>> {
    // Use the key string bytes directly
    let key_bytes = key.as_bytes();
    
    // Retrieve using low-level API
    match context.get_by_key(key_bytes) {
        Ok(value) => value,
        Err(_) => None,
    }
}

/// Delete a key
fn delete_impl(context: &mut Context, key: &str) -> bool {
    // Use the key string bytes directly
    let key_bytes = key.as_bytes();
    
    // For deletion, we'll just overwrite with an empty value
    // In a real implementation, we'd want a proper deletion mechanism
    match context.store_by_key(key_bytes, Vec::new()) {
        Ok(_) => true,
        Err(_) => false,
    }
}

// Direct export function for store
#[no_mangle]
pub extern "C" fn store_direct(key_ptr: *const u8, key_len: usize, value_ptr: *const u8, value_len: usize) -> bool {
    let mut ctx = Context::new();
    
    // Parse key string
    let key_str = unsafe {
        let key_slice = core::slice::from_raw_parts(key_ptr, key_len);
        core::str::from_utf8_unchecked(key_slice)
    };
    
    // Get value bytes
    let value = unsafe {
        core::slice::from_raw_parts(value_ptr, value_len)
    };
    
    // Call the implementation
    store_impl(&mut ctx, key_str, value)
}

// Direct export function for get
#[no_mangle]
pub extern "C" fn get_direct(key_ptr: *const u8, key_len: usize) -> *const u8 {
    let mut ctx = Context::new();
    
    // Parse key string
    let key_str = unsafe {
        let key_slice = core::slice::from_raw_parts(key_ptr, key_len);
        core::str::from_utf8_unchecked(key_slice)
    };
    
    // Get the value
    match get_impl(&mut ctx, key_str) {
        Some(value) => {
            if value.is_empty() {
                return core::ptr::null();
            }
            
            // IMPORTANT: In a real implementation, we would need to handle memory properly
            // For this test contract, we're leaking memory for simplicity
            let boxed = value.into_boxed_slice();
            Box::leak(boxed).as_ptr()
        },
        None => core::ptr::null(),
    }
}

// Direct export function for delete
#[no_mangle]
pub extern "C" fn delete_direct(key_ptr: *const u8, key_len: usize) -> bool {
    let mut ctx = Context::new();
    
    // Create a string from the key bytes
    let key_str = unsafe {
        let key_slice = core::slice::from_raw_parts(key_ptr, key_len);
        core::str::from_utf8_unchecked(key_slice)
    };
    
    delete_impl(&mut ctx, key_str)
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
        
        if parts.len() >= 2 {
            let command = parts[0];
            let key = parts[1];
            
            // Handle different commands
            if command == "store" && parts.len() >= 3 {
                let value = parts[2];
                
                // Execute the store operation
                let success = store_impl(&mut ctx, key, value.as_bytes());
                
                // Convert the result to a string
                let result_str = if success { "success" } else { "failed" };
                let result_bytes = result_str.as_bytes();
                
                // Set the result
                unsafe {
                    set_call_result(result_bytes.as_ptr(), result_bytes.len());
                }
                
                return;
            } else if command == "get" {
                // Execute the get operation
                if let Some(value) = get_impl(&mut ctx, key) {
                    // Use the value bytes directly
                    unsafe {
                        set_call_result(value.as_ptr(), value.len());
                    }
                } else {
                    // Key not found
                    let not_found = "not_found";
                    unsafe {
                        set_call_result(not_found.as_ptr(), not_found.len());
                    }
                }
                
                return;
            } else if command == "delete" {
                // Execute the delete operation
                let success = delete_impl(&mut ctx, key);
                
                // Convert the result to a string
                let result_str = if success { "success" } else { "failed" };
                let result_bytes = result_str.as_bytes();
                
                // Set the result
                unsafe {
                    set_call_result(result_bytes.as_ptr(), result_bytes.len());
                }
                
                return;
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
