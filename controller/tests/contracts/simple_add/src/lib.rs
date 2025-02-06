#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

pub mod types;
use types::AddParams;
use borsh::BorshDeserialize;
use core::fmt::Write;

#[cfg(target_arch = "wasm32")]
use alloc::string::String;

use wasmlanche::{public, Context};

/// Add two numbers together
#[public]
pub fn add(context: &mut Context, ptr: i32, len: i32) -> i64 {
    // Read params from memory
    let params = unsafe {
        let slice = core::slice::from_raw_parts(ptr as *const u8, len as usize);
        AddParams::try_from_slice(slice).expect("Failed to deserialize params")
    };
    
    let result = params.a + params.b;
    #[cfg(target_arch = "wasm32")]
    {
        let mut output = String::new();
        write!(&mut output, "Adding {} + {} = {}", params.a, params.b, result).unwrap();
        wasmlanche::log(&output);
    }
    result as i64
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
