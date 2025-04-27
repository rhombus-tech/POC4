/*!
 * Ahead-of-Time Compilation for NASDAQ ITCH Protocol Processing
 * 
 * This module provides AOT compilation capabilities to optimize high-frequency
 * market data processing within TEE environments. It focuses on:
 * 
 * 1. Binary message parsing optimization
 * 2. Parameter format specialization (length-prefixed and direct)
 * 3. WebAssembly contract pre-compilation
 * 4. Secure attestation with pre-compiled components
 */

mod compiler;
mod optimization;
mod parameter_format;
mod wasm;

pub use compiler::AotCompiler;
pub use parameter_format::{ParameterSpecialization, FormatDetection};
pub use wasm::WasmAotCompiler;

/// Compilation configuration for AOT optimization
#[derive(Debug, Clone)]
pub struct AotConfig {
    /// Enable specialized parameter format handlers
    pub parameter_format_specialization: bool,
    /// Enable message type specialization
    pub message_type_specialization: bool,
    /// Enable profile-guided optimization
    pub pgo_enabled: bool,
    /// Path to profile data for PGO
    pub profile_data_path: Option<String>,
    /// Optimization level (0-3)
    pub opt_level: u8,
    /// Maximum size of specialized message handlers (to avoid code bloat)
    pub max_specialized_handlers: usize,
}

impl Default for AotConfig {
    fn default() -> Self {
        Self {
            parameter_format_specialization: true,
            message_type_specialization: true,
            pgo_enabled: false,
            profile_data_path: None,
            opt_level: 3,
            max_specialized_handlers: 100,
        }
    }
}

/// Performance profile for different market conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketProfile {
    /// Normal trading conditions
    Normal,
    /// Market open with high message volume
    Opening,
    /// High volatility conditions
    HighVolatility,
    /// Low liquidity conditions
    LowLiquidity,
    /// Custom profile
    Custom,
}

/// AOT compilation trait for ITCH message processors
pub trait AotCompilable {
    /// Prepare for AOT compilation
    fn prepare(&mut self, config: &AotConfig) -> Result<(), crate::error::AdapterError>;
    
    /// Compile optimized handlers for specific message types
    fn compile_message_handlers(&mut self, config: &AotConfig) -> Result<(), crate::error::AdapterError>;
    
    /// Create specialized parameter format handlers
    fn compile_parameter_handlers(&mut self, config: &AotConfig) -> Result<(), crate::error::AdapterError>;
    
    /// Verify compiled code integrity for TEE execution
    fn verify_integrity(&self) -> Result<bool, crate::error::AdapterError>;
}
