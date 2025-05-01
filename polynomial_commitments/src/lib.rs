//! # Polynomial Commitment Scheme for ZK State Archival System
//! 
//! This crate implements polynomial commitments based on tensor encoding
//! as described in "The Accidental Computer: Polynomial Commitments from
//! Data Availability" (Evans & Angeris, 2025).
//!
//! The implementation is designed to integrate with our dual TEE architecture
//! and ZK state archival system for robust and secure storage reduction.
//!
//! ## Security Features
//!
//! ### Dual-Format Parameter Handling
//! 
//! This implementation supports two parameter passing patterns:
//!
//! 1. **Length-prefixed format**:
//!    - First 4 bytes represent a little-endian u32 length
//!    - Actual data follows the 4-byte length prefix
//!    - This is the common WebAssembly convention
//!
//! 2. **Direct data format**:
//!    - No length prefix, data is passed directly
//!    - Used primarily in tests and certain integration scenarios
//!    - Expected to be of a fixed, reasonable size
//!
//! Input parameter handling includes:
//! - Validation of the first 4 bytes as a potential length prefix
//! - Rejection of unreasonable lengths (e.g., greater than 1,000,000 elements)
//! - Fallback to direct format when appropriate
//! - Comprehensive debug logging for both cases
//! - Robust bounds checking to prevent memory vulnerabilities
//!
//! ### Protection Against Memory Vulnerabilities
//!
//! This implementation includes specific protections against memory-related vulnerabilities by:
//! - Validating parameter lengths before any memory allocation
//! - Rejecting unreasonable length prefixes (>1,000,000 elements)
//! - Implementing robust error handling that never panics on invalid input
//! - Using constant-time operations for cryptographic primitives
//! - Ensuring all array accesses are bounds-checked
//!
//! ### Field Element Comparison
//!
//! For modular field element comparisons (especially with pasta_curves::Fp), we use the robust
//! "difference equals zero" pattern to ensure correctness, as direct equality checks may not
//! work reliably with certain field implementations.

// Core implementation modules
pub mod tensor;
pub mod encoder;
pub mod sampler;
pub mod polynomial;
pub mod error;

// Only include TEE integration when feature is enabled
#[cfg(feature = "tee_integration")]
pub mod tee_integration;

// Public exports
pub use tensor::{TensorEncoder, Matrix};
pub use encoder::Encoder;
pub use sampler::Sampler;
pub use polynomial::PolynomialCommitment;
pub use error::Error;

#[cfg(feature = "tee_integration")]
pub use tee_integration::TeeIntegration;

/// Re-export of field traits and implementations
pub mod field {
    pub use ff::{Field, PrimeField};
}

#[cfg(test)]
mod tests {
    use super::*;
    use pasta_curves::Fp;
    
    // Use pasta_curves::Fp for tests as it already implements all required Field traits
    // This aligns with our design goals for production-quality cryptography
    
    // Very basic test to ensure tensor operations work
    #[test]
    fn test_basic_tensor_operations() {
        let encoder = TensorEncoder::<Fp>::new();
        
        // Generate simple test vectors
        let r = vec![Fp::from(1u64), Fp::from(2u64)];
        let result = encoder.generate_randomness(2, &r).unwrap();
        
        assert_eq!(result.len(), 4);
        // We don't check specific values since we're just verifying it runs
    }
}
