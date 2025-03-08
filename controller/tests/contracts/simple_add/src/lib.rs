#![no_std]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

// Import just what we need from wasmlanche
use wasmlanche::Context;

// Define the external imports required by the WASM environment
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn set_call_result(ptr: *const u8, len: usize);
}

/// Add two numbers together - internal implementation
fn add_impl(_context: &mut Context, a: i32, b: i32) -> i32 {
    // Implementation that's shared by all entry points
    a + b
}

/// Read integer parameters from memory
/// Supports both length-prefixed and direct parameter formats
#[cfg(target_arch = "wasm32")]
fn read_input_params(input_offset: i32) -> (i32, i32) {
    // Safety bounds - don't try to read unreasonable amounts of memory
    const MAX_PARAM_SIZE: usize = 1024;
    
    unsafe {
        let input_ptr = input_offset as *const u8;
        
        // First check if this could be a length-prefixed format
        // by reading the first 4 bytes as a potential length
        let potential_length = if input_offset >= 4 {
            let length_bytes = core::slice::from_raw_parts(input_ptr, 4);
            u32::from_le_bytes([
                length_bytes[0], 
                length_bytes[1], 
                length_bytes[2], 
                length_bytes[3]
            ])
        } else {
            // If the offset is less than 4, we can't possibly have a length prefix
            0
        };
        
        // If the length seems reasonable, treat as length-prefixed format
        if potential_length > 0 && potential_length <= MAX_PARAM_SIZE as u32 {
            // This is likely a length-prefixed parameter
            // Read the actual parameters after the length prefix
            let param_ptr = input_ptr.add(4);
            let param_slice = core::slice::from_raw_parts(param_ptr, core::cmp::min(potential_length as usize, MAX_PARAM_SIZE));
            
            // Parse parameters (assuming comma-separated values like "42,58")
            if let Some(comma_pos) = param_slice.iter().position(|&b| b == b',') {
                let a_slice = &param_slice[..comma_pos];
                let b_slice = &param_slice[(comma_pos + 1)..];
                
                // Try to convert slices to strings and parse them
                if let (Ok(a_str), Ok(b_str)) = (
                    core::str::from_utf8(a_slice),
                    core::str::from_utf8(b_slice)
                ) {
                    if let (Ok(a), Ok(b)) = (a_str.parse::<i32>(), b_str.parse::<i32>()) {
                        return (a, b);
                    }
                }
            }
            
            // Fallback if comma parsing failed - assume single value
            if let Ok(param_str) = core::str::from_utf8(param_slice) {
                if let Ok(value) = param_str.parse::<i32>() {
                    return (value, 0); // Default second parameter to 0
                }
            }
        }
        
        // Direct parameter format - use the input_offset directly as first parameter
        // and provide a default for the second parameter
        (input_offset, 0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_input_params(input_offset: i32) -> (i32, i32) {
    // For non-wasm builds, just return the input as first parameter
    (input_offset, 0)
}

// Direct export function for add (without the 'export_' prefix)
// This matches what the Go tests are looking for
#[no_mangle]
pub extern "C" fn add_direct(input_offset: i32) -> i32 {
    // Create a new context
    let mut ctx = Context::new();
    
    // Read parameters using our helper function that handles both formats
    let (a, b) = read_input_params(input_offset);
    
    // Call the implementation
    add_impl(&mut ctx, a, b)
}

// This is the function matching the signature expected by Go tests
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn wasm_add(a: i32, b: i32) -> i32 {
    let mut ctx = Context::new();
    let result = add_impl(&mut ctx, a, b);
    
    // Convert result to bytes for set_call_result
    let result_bytes = result.to_le_bytes();
    
    unsafe {
        // Set the call result using the imported function
        set_call_result(result_bytes.as_ptr(), result_bytes.len());
    }
    
    result
}

// Original style export for Rust-side compatibility
#[no_mangle]
pub extern "C" fn export_add(ctx_ptr: *mut Context, a: i32, b: i32) -> i32 {
    unsafe {
        let ctx = &mut *ctx_ptr;
        add_impl(ctx, a, b)
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
