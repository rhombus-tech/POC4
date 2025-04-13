#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

/// Panic handler for the WebAssembly contract
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

/// Allocation error handler for the WebAssembly contract
#[alloc_error_handler]
fn alloc_error(_: Layout) -> ! {
    loop {}
}

/// Market Data Consumer Contract
/// 
/// Example contract that consumes NASDAQ ITCH market data from the TEE mesh.
/// Demonstrates how to access order book data and calculate market metrics.

// ITCH order book structures mirroring client-side definitions
#[derive(Serialize, Deserialize, Debug)]
pub struct OrderBookEntry {
    pub price: f64,
    pub size: u32,
    pub order_count: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OrderBook {
    pub symbol: String,
    pub timestamp: u64,
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MarketMetrics {
    pub symbol: String,
    pub bid_ask_spread: f64,
    pub spread_percentage: f64,
    pub midpoint_price: f64,
    pub market_depth_usd: f64,
    pub bid_depth: u32,
    pub ask_depth: u32,
    pub timestamp: u64,
}

// External functions provided by the TEE runtime
extern "C" {
    fn alloc_memory(size: u32, align: u32) -> u32;
    fn free_memory(ptr: u32, size: u32);
    fn log_message(msg_ptr: u32, msg_len: u32);
}

/// Log a debug message to the TEE environment
fn debug_log(message: &str) {
    unsafe {
        let bytes = message.as_bytes();
        let ptr = ALLOCATOR.alloc(Layout::from_size_align(bytes.len(), 1).unwrap());
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        log_message(ptr as u32, bytes.len() as u32);
        ALLOCATOR.dealloc(ptr, Layout::from_size_align(bytes.len(), 1).unwrap());
    }
}

/// Main entry point for processing ITCH market data
/// 
/// This function is called by the TEE environment to process market data
/// received through the NASDAQ ITCH feed. It handles both parameter formats:
/// - Length-prefixed: data starts with a 4-byte length prefix
/// - Direct: data is passed directly
#[no_mangle]
pub extern "C" fn process_itch_data(data_ptr: u32, data_len: u32) -> u32 {
    debug_log("Starting ITCH data processing in TEE environment");
    
    // Handle the parameter format
    let data = get_parameter_data(data_ptr, data_len);
    
    // Process the ITCH data
    let result = process_data(&data);
    
    debug_log(&format!("ITCH processing complete, returning {} bytes", result.len()));
    
    // Return the result as a pointer to the TEE environment
    return_result(result)
}

/// Process the ITCH market data
/// 
/// This function performs the actual market data analysis using the ITCH protocol.
/// In a production environment, this would implement sophisticated market analysis
/// like order book reconstruction, price trends, etc.
fn process_data(data: &[u8]) -> Vec<u8> {
    // Parse ITCH message types and perform analysis
    
    // First byte typically indicates message type in ITCH protocol
    let message_type = if !data.is_empty() { data[0] } else { 0 };
    
    debug_log(&format!("Processing ITCH message type: {}", message_type));
    
    // Basic parsing of common ITCH message types
    let analysis_result = match message_type {
        // Add Order Message (message type 'A')
        b'A' => {
            if data.len() >= 36 {  // Ensure we have enough data for an Add Order message
                let price_offset = 32;
                let mut price_bytes = [0u8; 4];
                price_bytes.copy_from_slice(&data[price_offset..price_offset+4]);
                let price = u32::from_be_bytes(price_bytes);
                
                format!("Add Order: Price={}", price / 10000.0)
            } else {
                "Incomplete Add Order message".to_string()
            }
        },
        
        // Order Executed Message (message type 'E')
        b'E' => {
            if data.len() >= 30 {  // Ensure we have enough data
                let executed_shares_offset = 20;
                let mut shares_bytes = [0u8; 4];
                shares_bytes.copy_from_slice(&data[executed_shares_offset..executed_shares_offset+4]);
                let shares = u32::from_be_bytes(shares_bytes);
                
                format!("Order Executed: Shares={}", shares)
            } else {
                "Incomplete Order Executed message".to_string()
            }
        },
        
        // Order Delete Message (message type 'D')
        b'D' => "Order Delete".to_string(),
        
        // System Event Message (message type 'S') 
        b'S' => "System Event".to_string(),
        
        // Price Level Update (message type 'U')
        b'U' => "Price Level Update".to_string(),
        
        // Default case for other message types
        _ => format!("Unhandled message type: {}", message_type),
    };
    
    // Format the results in JSON for easy consumption by TEE consumer
    let mut result = Vec::with_capacity(analysis_result.len() + 20);
    result.extend_from_slice(b"{\"analysis\":\"");
    result.extend_from_slice(analysis_result.as_bytes());
    result.extend_from_slice(b"\",\"success\":true}");
    
    result
}

/// Get parameter data from the TEE environment
fn get_parameter_data(data_ptr: u32, data_len: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity(data_len as usize);
    
    // Check for length-prefixed format (4 bytes length + data)
    let maybe_len = if data_len >= 4 {
        let ptr = data_ptr as *const u32;
        unsafe { *ptr }
    } else {
        0
    };
    
    if maybe_len > 0 && maybe_len < 1_000_000 {
        // Length-prefixed format
        let data_ptr = (data_ptr as usize + 4) as *const u8;
        let data = unsafe { core::slice::from_raw_parts(data_ptr, maybe_len as usize) };
        data.to_vec()
    } else {
        // Direct format - assume it's a serialized order book
        // Max size 64KB to prevent overflow
        let data_ptr = data_ptr as *const u8;
        let data = unsafe { core::slice::from_raw_parts(data_ptr, data_len as usize) };
        data.to_vec()
    }
}

/// Return the result as a pointer to the TEE environment
fn return_result(result: Vec<u8>) -> u32 {
    let result_len = result.len() as u32;
    let result_ptr = alloc_memory(result_len as u32 + 4, 4);
    
    unsafe {
        // Write length prefix (little endian)
        *(result_ptr as *mut u32) = result_len;
        
        // Write data
        let data_ptr = (result_ptr as usize + 4) as *mut u8;
        core::ptr::copy_nonoverlapping(result.as_ptr(), data_ptr, result_len as usize);
    }
    
    result_ptr
}

/// Simple allocator for the WASM module
struct SimpleAllocator;

unsafe impl GlobalAlloc for SimpleAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let ptr = alloc_memory(size as u32, align as u32);
        ptr as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        free_memory(ptr as u32, size as u32);
    }
    
    ptr
}

/// Contract initialization
#[no_mangle]
pub extern "C" fn init(_params_ptr: u32) -> u32 {
    // No initialization needed
    1
}
