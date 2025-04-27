// RSA Accumulator - Simplified version for Enarx TEE execution
// Supports dual-format parameter validation (length-prefixed and direct formats)

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};

// Constants for parameter validation
const MAX_PARAM_SIZE: usize = 1024; // Default max parameter size (can be overridden by env)
const LENGTH_PREFIX_SIZE: usize = 4; // Size of length prefix (u32)
const CONTRACT_ID_SIZE: usize = 32;  // Size of direct format contract IDs

// Global accumulator state
struct AccumulatorState {
    accumulated_params: Vec<[u8; 32]>,
    current_hash: [u8; 32],
    total_processed: u64,
    length_prefixed_count: u64,
    direct_format_count: u64,
}

// Error type for parameter validation
#[derive(Debug)]
enum AccumulatorError {
    InvalidFormat,
    SizeTooLarge,
    InvalidData,
}

// Result type alias
type AccumResult<T> = Result<T, AccumulatorError>;

// Main entry point for the WebAssembly module
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize accumulator state
    let state = Arc::new(Mutex::new(AccumulatorState {
        accumulated_params: Vec::new(),
        current_hash: [0; 32],
        total_processed: 0,
        length_prefixed_count: 0,
        direct_format_count: 0,
    }));
    
    // Read environment variables
    let max_param_size = env::var("MAX_PARAMETER_SIZE")
        .map(|v| v.parse::<usize>().unwrap_or(MAX_PARAM_SIZE))
        .unwrap_or(MAX_PARAM_SIZE);
    
    let enable_length_prefix = env::var("ENABLE_LENGTH_PREFIX_FORMAT")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(true);
    
    let enable_direct_format = env::var("ENABLE_DIRECT_FORMAT")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(true);
    
    // Connect to Unix domain socket for IPC
    let socket_path = "/tmp/accumulator.sock";
    
    // Main processing loop
    loop {
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                // Read request
                let mut buffer = [0; 4096];
                match stream.read(&mut buffer) {
                    Ok(size) if size > 0 => {
                        let data = &buffer[0..size];
                        
                        // Process the data
                        let result = process_parameter(
                            data, 
                            &state,
                            max_param_size,
                            enable_length_prefix,
                            enable_direct_format
                        );
                        
                        // Send response
                        let response = match result {
                            Ok((validated, format)) => {
                                // Update statistics
                                let mut state_lock = state.lock().unwrap();
                                state_lock.total_processed += 1;
                                
                                if format == "length-prefixed" {
                                    state_lock.length_prefixed_count += 1;
                                } else {
                                    state_lock.direct_format_count += 1;
                                }
                                
                                // Add to accumulator
                                state_lock.accumulated_params.push(validated);
                                
                                // In a real implementation, this would update the RSA accumulator
                                
                                // Generate success response
                                format!("{{\"success\":true,\"format\":\"{}\",\"hash\":\"{}\"}}",
                                    format,
                                    hex::encode(&state_lock.current_hash)
                                )
                            },
                            Err(e) => {
                                // Generate error response
                                format!("{{\"success\":false,\"error\":\"{:?}\"}}", e)
                            }
                        };
                        
                        // Write response
                        if let Err(e) = stream.write_all(response.as_bytes()) {
                            eprintln!("Error writing response: {:?}", e);
                        }
                    },
                    Ok(_) => {
                        // Empty read, do nothing
                    },
                    Err(e) => {
                        eprintln!("Error reading from socket: {:?}", e);
                    }
                }
            },
            Err(e) => {
                eprintln!("Error connecting to socket: {:?}", e);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
}

// Process a parameter with dual-format validation
fn process_parameter(
    data: &[u8],
    state: &Arc<Mutex<AccumulatorState>>,
    max_size: usize,
    enable_length_prefix: bool,
    enable_direct_format: bool
) -> AccumResult<([u8; 32], String)> {
    // Validate parameter format
    parse_dual_format_parameters(
        data,
        max_size,
        enable_length_prefix,
        enable_direct_format
    )
}

// Dual-format parameter validation
// Supports both length-prefixed and direct formats
fn parse_dual_format_parameters(
    data: &[u8],
    max_size: usize,
    enable_length_prefix: bool,
    enable_direct_format: bool
) -> AccumResult<([u8; 32], String)> {
    // Check if length-prefixed format is enabled and data is long enough
    if enable_length_prefix && data.len() >= LENGTH_PREFIX_SIZE {
        // Extract length prefix (little-endian u32)
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&data[0..LENGTH_PREFIX_SIZE]);
        let length = u32::from_le_bytes(length_bytes) as usize;
        
        // Validate length
        if length > 0 && length <= max_size && data.len() >= LENGTH_PREFIX_SIZE + length {
            // Extract parameter data
            let param_data = &data[LENGTH_PREFIX_SIZE..LENGTH_PREFIX_SIZE+length];
            
            // Copy to fixed-size array
            return Ok((copy_to_fixed_array(param_data)?, "length-prefixed".into()));
        }
    }
    
    // If direct format is enabled, try that
    if enable_direct_format {
        // For direct format, we expect exactly CONTRACT_ID_SIZE bytes
        if data.len() == CONTRACT_ID_SIZE {
            // Copy directly to fixed-size array
            let mut result = [0u8; 32];
            result.copy_from_slice(data);
            return Ok((result, "direct".into()));
        }
    }
    
    // If we get here, neither format was valid
    Err(AccumulatorError::InvalidFormat)
}

// Helper to copy data to a fixed-size array with proper bounds checking
fn copy_to_fixed_array(data: &[u8]) -> AccumResult<[u8; 32]> {
    let mut result = [0u8; 32];
    
    // Ensure we have enough data
    if data.len() < CONTRACT_ID_SIZE {
        return Err(AccumulatorError::InvalidData);
    }
    
    // Copy the data (bounded to 32 bytes)
    let copy_len = std::cmp::min(data.len(), CONTRACT_ID_SIZE);
    result[0..copy_len].copy_from_slice(&data[0..copy_len]);
    
    Ok(result)
}

// For testing - Export functions that don't use Unix sockets
#[cfg(test)]
mod exports {
    use super::*;
    
    #[no_mangle]
    pub extern "C" fn validate_parameter(ptr: *const u8, len: usize) -> i32 {
        let data = unsafe { std::slice::from_raw_parts(ptr, len) };
        
        match parse_dual_format_parameters(
            data,
            MAX_PARAM_SIZE,
            true,  // Enable length-prefixed
            true   // Enable direct format
        ) {
            Ok(_) => 1,  // Success
            Err(_) => 0, // Failure
        }
    }
}
