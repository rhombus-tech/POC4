// NASDAQ ITCH Orderbook Analyzer - WebAssembly Contract for TEE
// This contract analyzes market data securely within the TEE environment

// Configure minimal std environment for WebAssembly
#![no_std]
#![allow(unused_variables)]

// Use panic_abort for panic handling
extern crate panic_abort;

// Use wee_alloc as the global allocator to minimize code size
extern crate wee_alloc;
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Memory management utilities for WebAssembly
mod memory {
    // Read a byte from Wasm linear memory
    pub unsafe fn read_byte(offset: usize) -> u8 {
        *(offset as *const u8)
    }

    // Read 4 bytes from memory as u32 (little-endian)
    pub unsafe fn read_u32(offset: usize) -> u32 {
        let bytes = [
            read_byte(offset),
            read_byte(offset + 1),
            read_byte(offset + 2),
            read_byte(offset + 3),
        ];
        u32::from_le_bytes(bytes)
    }

    // Read 8 bytes from memory as u64 (little-endian)
    pub unsafe fn read_u64(offset: usize) -> u64 {
        let bytes = [
            read_byte(offset),
            read_byte(offset + 1),
            read_byte(offset + 2),
            read_byte(offset + 3),
            read_byte(offset + 4),
            read_byte(offset + 5),
            read_byte(offset + 6),
            read_byte(offset + 7),
        ];
        u64::from_le_bytes(bytes)
    }

    // Write u64 result to a memory location
    pub unsafe fn write_u64(offset: usize, value: u64) {
        let bytes = value.to_le_bytes();
        for i in 0..8 {
            *(((offset + i) as *mut u8)) = bytes[i];
        }
    }
}

// Parameter format detection
fn detect_parameter_format(ptr: usize, len: usize) -> ParameterFormat {
    // If too short for a length prefix, must be direct format
    if len < 4 {
        return ParameterFormat::Direct;
    }

    // Check for length-prefixed format by reading first 4 bytes
    unsafe {
        let declared_len = memory::read_u32(ptr);
        
        // If the declared length makes sense (0 < len <= 1024)
        // and matches the actual parameter length, it's likely length-prefixed
        if declared_len > 0 && declared_len <= 1024 && 
           (declared_len as usize) == len - 4 {
            ParameterFormat::LengthPrefixed
        } else {
            // Otherwise, treat as direct format
            ParameterFormat::Direct
        }
    }
}

// Parameter format types
enum ParameterFormat {
    // First 4 bytes are length, followed by data
    LengthPrefixed,
    // Raw data, no length prefix
    Direct,
}

// Message type constants (based on NASDAQ ITCH protocol)
const MSG_TYPE_ADD_ORDER: u8 = b'A';
const MSG_TYPE_ORDER_EXECUTED: u8 = b'E';
const MSG_TYPE_ORDER_CANCEL: u8 = b'X';
const MSG_TYPE_TRADE: u8 = b'P';

// Basic analysis of ITCH data within the TEE
#[no_mangle]
pub extern "C" fn analyze_orderbook(params_ptr: i32, params_len: i32) -> i64 {
    // Cast parameters to usize for memory operations
    let ptr = params_ptr as usize;
    let len = params_len as usize;

    // Validate parameters
    if len == 0 {
        return error_code(1, "Empty parameters");
    }

    // Detect parameter format (length-prefixed vs direct)
    let format = detect_parameter_format(ptr, len);

    // Process based on detected format
    match format {
        ParameterFormat::LengthPrefixed => {
            // Skip the 4-byte length prefix
            process_messages(ptr + 4, len - 4)
        },
        ParameterFormat::Direct => {
            // Process directly
            process_messages(ptr, len)
        }
    }
}

// Process ITCH messages in TEE
fn process_messages(data_ptr: usize, data_len: usize) -> i64 {
    // Initialize counters for different message types
    let mut add_order_count: u64 = 0;
    let mut execute_count: u64 = 0;
    let mut cancel_count: u64 = 0;
    let mut trade_count: u64 = 0;
    
    // Initialize trading metrics
    let mut total_volume: u64 = 0;
    let mut total_value: u64 = 0; // Price * Volume
    let mut max_price: u64 = 0;
    let mut min_price: u64 = u64::MAX;

    // Parse all messages in the buffer
    let mut pos = 0;
    while pos + 1 < data_len {
        // Read message type
        let msg_type = unsafe { memory::read_byte(data_ptr + pos) };
        pos += 1;

        match msg_type {
            MSG_TYPE_ADD_ORDER => {
                // Process Add Order message
                add_order_count += 1;
                
                // Message format: Type(1) + Size(4) + Price(8) + Side(1)
                if pos + 13 <= data_len {
                    // Read price and update metrics
                    let price = unsafe { memory::read_u64(data_ptr + pos + 4) };
                    let size = unsafe { memory::read_u32(data_ptr + pos) };
                    
                    // Update price metrics
                    if price > 0 {
                        if price > max_price {
                            max_price = price;
                        }
                        if price < min_price {
                            min_price = price;
                        }
                    }
                    
                    pos += 13;
                } else {
                    // Skip malformed message
                    pos = data_len;
                }
            },
            MSG_TYPE_ORDER_EXECUTED => {
                // Process Order Executed message
                execute_count += 1;
                
                // Message format: Type(1) + Size(4) + Price(8)
                if pos + 12 <= data_len {
                    let size = unsafe { memory::read_u32(data_ptr + pos) };
                    let price = unsafe { memory::read_u64(data_ptr + pos + 4) };
                    
                    // Update volume and value
                    total_volume += size as u64;
                    total_value += price * size as u64;
                    
                    pos += 12;
                } else {
                    pos = data_len;
                }
            },
            MSG_TYPE_ORDER_CANCEL => {
                // Process Order Cancel message
                cancel_count += 1;
                
                // Message format: Type(1) + Size(4)
                if pos + 4 <= data_len {
                    pos += 4;
                } else {
                    pos = data_len;
                }
            },
            MSG_TYPE_TRADE => {
                // Process Trade message
                trade_count += 1;
                
                // Message format: Type(1) + Size(4) + Price(8)
                if pos + 12 <= data_len {
                    let size = unsafe { memory::read_u32(data_ptr + pos) };
                    let price = unsafe { memory::read_u64(data_ptr + pos + 4) };
                    
                    // Update volume and value (for trades)
                    total_volume += size as u64;
                    total_value += price * size as u64;
                    
                    pos += 12;
                } else {
                    pos = data_len;
                }
            },
            _ => {
                // Skip unknown message type
                pos += 1;
            }
        }
    }

    // Calculate market imbalance indicator
    // A basic calculation: (add_orders - (executes + cancels)) / total_messages
    let total_messages = add_order_count + execute_count + cancel_count + trade_count;
    let imbalance = if total_messages > 0 {
        (((add_order_count as i64) - ((execute_count + cancel_count) as i64)) * 10000) / (total_messages as i64)
    } else {
        0
    };

    // Calculate average price
    let avg_price = if total_volume > 0 {
        total_value / total_volume
    } else {
        0
    };

    // Store results in a globally accessible memory location
    // This is a simplified example - real contracts would use a more structured approach
    unsafe {
        // Store results at fixed memory locations for TEE to read
        memory::write_u64(10000, add_order_count);
        memory::write_u64(10008, execute_count);
        memory::write_u64(10016, cancel_count);
        memory::write_u64(10024, trade_count);
        memory::write_u64(10032, total_volume);
        memory::write_u64(10040, avg_price);
        memory::write_u64(10048, max_price);
        memory::write_u64(10056, min_price);
        memory::write_u64(10064, imbalance as u64);
    }

    // Return success (0) and imbalance indicator as high bits
    ((imbalance as i64) << 32) | 0
}

// Error handling
fn error_code(code: i32, _msg: &str) -> i64 {
    // In a production contract, we would log the error message
    // Return negative code to indicate error
    -(code as i64)
}

// Export the entry point for WebAssembly execution
#[no_mangle]
pub extern "C" fn entry_point(params_ptr: i32, params_len: i32) -> i64 {
    analyze_orderbook(params_ptr, params_len)
}
