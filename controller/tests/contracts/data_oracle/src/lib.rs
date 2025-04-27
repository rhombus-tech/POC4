#![no_std]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

#[cfg(target_arch = "wasm32")]
use alloc::{string::{String, ToString}, vec::Vec};

// Import what we need from wasmlanche
use wasmlanche::Context;

// Define the external imports required by the WASM environment
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn set_call_result(ptr: *const u8, len: usize);
}

// Hardcoded constants for storage
const PRICE_PREFIX: &[u8] = b"price_";

/// Query price data for an asset symbol
fn query_price_impl(context: &mut Context, symbol: &str) -> Option<u64> {
    // Create a key combining the prefix and the symbol
    let mut key = Vec::with_capacity(PRICE_PREFIX.len() + symbol.len());
    key.extend_from_slice(PRICE_PREFIX);
    key.extend_from_slice(symbol.as_bytes());
    
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

/// Set price data for an asset
fn set_price_impl(context: &mut Context, symbol: &str, price: u64) -> bool {
    // Create a key combining the prefix and the symbol
    let mut key = Vec::with_capacity(PRICE_PREFIX.len() + symbol.len());
    key.extend_from_slice(PRICE_PREFIX);
    key.extend_from_slice(symbol.as_bytes());
    
    // Store the price as bytes
    let value = price.to_le_bytes().to_vec();
    
    // Store the data directly
    match context.store_by_key(&key, value) {
        Ok(_) => true,
        Err(_) => false,
    }
}

// Direct export function for querying price
#[no_mangle]
pub extern "C" fn query_price_direct(symbol_ptr: *const u8, symbol_len: usize) -> u64 {
    let mut ctx = Context::new();
    
    // Parse symbol string
    let symbol = unsafe {
        let symbol_slice = core::slice::from_raw_parts(symbol_ptr, symbol_len);
        core::str::from_utf8_unchecked(symbol_slice)
    };
    
    // Call the implementation
    query_price_impl(&mut ctx, symbol).unwrap_or(0)
}

// Direct export function for setting price
#[no_mangle]
pub extern "C" fn set_price_direct(symbol_ptr: *const u8, symbol_len: usize, price: u64) -> bool {
    let mut ctx = Context::new();
    
    // Create a string from the symbol bytes
    let symbol_str = unsafe {
        let symbol_slice = core::slice::from_raw_parts(symbol_ptr, symbol_len);
        core::str::from_utf8_unchecked(symbol_slice)
    };
    
    set_price_impl(&mut ctx, symbol_str, price)
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
            let symbol = parts[1];
            
            // Handle different commands
            if command == "set_price" && parts.len() >= 3 {
                // Parse price
                if let Ok(price) = parts[2].parse::<u64>() {
                    // Execute the set price operation
                    let success = set_price_impl(&mut ctx, symbol, price);
                    
                    // Convert the result to a string
                    let result_str = if success { "success" } else { "failed" };
                    let result_bytes = result_str.as_bytes();
                    
                    // Set the result
                    unsafe {
                        set_call_result(result_bytes.as_ptr(), result_bytes.len());
                    }
                    
                    return;
                }
            } else if command == "query_price" {
                // Execute the query price operation
                if let Some(price) = query_price_impl(&mut ctx, symbol) {
                    // Convert to string
                    let result_str = price.to_string();
                    let result_bytes = result_str.as_bytes();
                    
                    // Set the result
                    unsafe {
                        set_call_result(result_bytes.as_ptr(), result_bytes.len());
                    }
                } else {
                    // Price not found
                    let not_found = "not_found";
                    unsafe {
                        set_call_result(not_found.as_ptr(), not_found.len());
                    }
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
