/*!
 * Protocol module for cross-regional TEE communication
 * 
 * This module implements the type-safe protocol for communication between
 * TEE pairs in different regions, supporting the verification mechanisms
 * demonstrated in the cross-regional test framework.
 * 
 * The implementation includes an optimized TEE-agnostic binary transport layer
 * that supports both Intel SGX and AMD SEV environments with efficient parameter
 * handling for WebAssembly contracts.
 */

pub mod binary_protocol;
pub mod client;
pub mod types;
pub mod tee_transport;
pub mod wasm_parameters;

// Re-export key components
pub use types::ParameterFormat;
pub use wasm_parameters::WasmParameterHandler;
pub use client::ProtocolClient;
pub use types::*;

use std::sync::Arc;
use crate::ClientConfig;
