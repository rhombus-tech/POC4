#![no_std]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

use wasmlanche::Context;

// Define the external imports required for WebAssembly environment
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn set_call_result(ptr: *const u8, len: usize);
}

// Internal implementation functions
fn add_impl(_context: &mut Context, a: i32, b: i32) -> i32 {
    a + b
}

fn execute_impl(_context: &mut Context) -> i32 {
    42
}

// Direct export function for add without the 'export_' prefix
#[no_mangle]
pub extern "C" fn add_direct(input_offset: i32) -> i32 {
    let mut ctx = Context::new();
    // Use input_offset as first parameter and a fixed value as second parameter
    let a = input_offset;
    let b = 2;
    add_impl(&mut ctx, a, b)
}

// Direct export function for execute without the 'export_' prefix
#[no_mangle]
pub extern "C" fn execute_direct(_input_offset: i32) -> i32 {
    let mut ctx = Context::new();
    execute_impl(&mut ctx)
}

// Functions matching the signature expected by Go tests with set_call_result
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn wasm_add(a: i32, b: i32) -> i32 {
    let mut ctx = Context::new();
    let result = add_impl(&mut ctx, a, b);
    
    let result_bytes = result.to_le_bytes();
    unsafe {
        set_call_result(result_bytes.as_ptr(), result_bytes.len());
    }
    
    result
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn wasm_execute() -> i32 {
    let mut ctx = Context::new();
    let result = execute_impl(&mut ctx);
    
    let result_bytes = result.to_le_bytes();
    unsafe {
        set_call_result(result_bytes.as_ptr(), result_bytes.len());
    }
    
    result
}

// Original style exports for Rust-side compatibility
#[no_mangle]
pub extern "C" fn export_add(ctx_ptr: *mut Context, a: i32, b: i32) -> i32 {
    unsafe {
        let ctx = &mut *ctx_ptr;
        add_impl(ctx, a, b)
    }
}

#[no_mangle]
pub extern "C" fn export_execute(ctx_ptr: *mut Context) -> i32 {
    unsafe {
        let ctx = &mut *ctx_ptr;
        execute_impl(ctx)
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
