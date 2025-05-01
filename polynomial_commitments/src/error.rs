//! Error types for polynomial commitments
//!
//! This module defines the error types used throughout the polynomial
//! commitment implementation, with specific focus on supporting the
//! dual-format parameter handling pattern to prevent the 3.5B byte vulnerability.

use std::fmt::Debug;
use thiserror::Error;

/// Errors that can occur in polynomial commitment operations
/// 
/// Generic over the field type for better type safety
#[derive(Error, Debug)]
pub enum Error<F: Debug> {
    /// Input validation error - occurs when validating input parameters
    /// Examples: empty matrices, null inputs
    #[error("Input validation error: {0}")]
    InputValidation(String),
    
    /// Error during tensor encoding operations
    /// Examples: matrix dimension mismatch, incompatible shapes
    #[error("Tensor encoding error: {0}")]
    TensorEncoding(String),
    
    /// Error during sampling verification
    /// Examples: inconsistent proofs, invalid format
    #[error("Sampling verification error: {0}")]
    SamplingVerification(String),
    
    /// Error during polynomial evaluation
    /// Examples: incompatible evaluation points
    #[error("Polynomial evaluation error: {0}")]
    PolynomialEvaluation(String),
    
    /// Parameter format error - specifically for dual-format parameter handling
    /// Critical for preventing the 3.5B byte vulnerability
    /// Examples: unreasonable lengths (>1024, 3.5B bytes), inconsistent formats
    #[error("Parameter format error: {0}")]
    ParameterFormat(String),
    
    /// Generic parameter errors with specific field elements
    /// Examples: invalid field elements, zero divisors
    #[error("Field parameter error: {:?}", .0)]
    FieldParameter(F),
    
    /// TEE integration error - specifically for secure enclave operations
    /// Examples: attestation failures, cross-validation errors
    #[error("TEE integration error: {0}")]
    TeeIntegration(String),
}
