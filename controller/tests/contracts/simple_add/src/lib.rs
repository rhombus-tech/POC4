#![no_std]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

use wasmlanche::{Context, public};

/// Add two numbers together
#[public]
pub fn add(_context: &mut Context, ptr: i32, len: i32) -> i32 {
    // Use context if needed
    ptr + len
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(target_arch = "wasm32")]
pub fn register_panic() {
    use core::panic::PanicInfo;
    #[panic_handler]
    fn panic(_info: &PanicInfo) -> ! {
        loop {}
    }
}
