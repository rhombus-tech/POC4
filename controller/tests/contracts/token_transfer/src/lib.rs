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

// Simple hard-coded storage key prefixes
const BALANCE_PREFIX: &[u8] = b"balance_";

/// Get balance for an address
fn get_balance(context: &mut Context, address: &[u8; 32]) -> u64 {
    // Create a key combining the prefix and the address
    let mut key = Vec::with_capacity(BALANCE_PREFIX.len() + address.len());
    key.extend_from_slice(BALANCE_PREFIX);
    key.extend_from_slice(address);
    
    // Try to get data directly using low-level API
    // In a real contract, we would handle errors properly
    match context.get_by_key(&key) {
        Ok(Some(value)) => {
            if value.len() >= 8 {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&value[0..8]);
                u64::from_le_bytes(bytes)
            } else {
                0
            }
        },
        _ => 0,
    }
}

/// Set balance for an address
fn set_balance(context: &mut Context, address: &[u8; 32], amount: u64) {
    // Create a key combining the prefix and the address
    let mut key = Vec::with_capacity(BALANCE_PREFIX.len() + address.len());
    key.extend_from_slice(BALANCE_PREFIX);
    key.extend_from_slice(address);
    
    // Store the amount as bytes
    let value = amount.to_le_bytes().to_vec();
    
    // Store the data directly using low-level API
    let _ = context.store_by_key(&key, value);
}

/// Transfer tokens from sender to recipient
fn transfer_impl(context: &mut Context, to: &[u8; 32], amount: u64) -> bool {
    // In a real contract, we would get the sender from context
    // For this test contract, we'll use a fixed sender address
    let sender = [1u8; 32];
    
    let sender_balance = get_balance(context, &sender);
    if sender_balance < amount {
        return false;
    }
    
    // Deduct from sender
    set_balance(context, &sender, sender_balance - amount);
    
    // Add to recipient
    let recipient_balance = get_balance(context, to);
    set_balance(context, to, recipient_balance + amount);
    
    true
}

// Direct export function for transfer
#[no_mangle]
pub extern "C" fn transfer_direct(to_ptr: *const u8, to_len: usize, amount: u64) -> bool {
    let mut ctx = Context::new();
    
    // Create address from raw bytes
    let mut address = [0u8; 32];
    unsafe {
        let to_slice = core::slice::from_raw_parts(to_ptr, to_len);
        let copy_len = core::cmp::min(to_len, 32);
        address[..copy_len].copy_from_slice(&to_slice[..copy_len]);
    }
    
    // Call the implementation
    transfer_impl(&mut ctx, &address, amount)
}

// Function for balance queries
#[no_mangle]
pub extern "C" fn balance_of(address_ptr: *const u8, address_len: usize) -> u64 {
    let mut ctx = Context::new();
    
    let address_bytes = unsafe {
        core::slice::from_raw_parts(address_ptr, address_len)
    };
    
    // Ensure we have a valid 32-byte address
    if address_bytes.len() != 32 {
        return 0;
    }
    
    let mut address = [0u8; 32];
    address.copy_from_slice(address_bytes);
    
    get_balance(&mut ctx, &address)
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
            let recipient_str = parts[1];
            
            // Handle different commands
            if command == "transfer" {
                // For simplicity, create a pseudo-hash of the recipient string
                let mut recipient = [0u8; 32];
                let recipient_bytes = recipient_str.as_bytes();
                let copy_len = core::cmp::min(recipient_bytes.len(), 32);
                recipient[..copy_len].copy_from_slice(&recipient_bytes[..copy_len]);
                
                // Parse amount
                if let Ok(amount) = parts[2].parse::<u64>() {
                    // Execute the transfer
                    let success = transfer_impl(&mut ctx, &recipient, amount);
                    
                    // Convert the result to a string
                    let result_str = if success { "success" } else { "failed" };
                    let result_bytes = result_str.as_bytes();
                    
                    // Set the result
                    unsafe {
                        set_call_result(result_bytes.as_ptr(), result_bytes.len());
                    }
                    
                    return;
                }
            } else if command == "balance" {
                // For simplicity, create a pseudo-hash of the address string
                let mut address = [0u8; 32];
                let address_bytes = recipient_str.as_bytes();
                let copy_len = core::cmp::min(address_bytes.len(), 32);
                address[..copy_len].copy_from_slice(&address_bytes[..copy_len]);
                
                // Get the balance
                let balance = get_balance(&mut ctx, &address);
                
                // Convert to string
                let result_str = balance.to_string();
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
