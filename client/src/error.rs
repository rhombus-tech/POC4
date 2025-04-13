/*!
 * Error types for the Aristo client library
 */

use thiserror::Error;

/// Errors that can occur when using the Aristo client
#[derive(Error, Debug)]
pub enum ClientError {
    /// Errors related to protocol communication
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    
    /// Errors related to external API adapters
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError),
    
    /// Errors related to attestation verification
    #[error("Attestation error: {0}")]
    Attestation(#[from] AttestationError),
    
    /// Errors related to serialization/deserialization
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Other errors
    #[error("{0}")]
    Other(String),
}

/// Errors that can occur during protocol operations
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// The specified region was not found
    #[error("Region not found: {0}")]
    RegionNotFound(String),
    
    /// Communication with the region failed
    #[error("Region communication error: {0}")]
    RegionCommunication(String),
    
    /// Request timed out
    #[error("Request timed out")]
    Timeout,
    
    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),
    
    /// Attestation error
    #[error("Attestation error: {0}")]
    Attestation(String),

    /// Protocol violation
    #[error("Protocol violation: {0}")]
    Violation(String),
    
    /// Cross-regional verification failed
    #[error("Cross-regional verification failed: {0}")]
    VerificationFailed(String),
    
    /// Parameter format error
    #[error("Parameter format error: {0}")]
    ParameterFormat(String),
}

/// Errors that can occur when using external API adapters
#[derive(Error, Debug)]
pub enum AdapterError {
    /// The adapter is not configured
    #[error("Adapter not configured: {0}")]
    NotConfigured(String),
    
    /// API request failed
    #[error("API request failed: {0}")]
    RequestFailed(String),
    
    /// API response parsing failed
    #[error("API response parsing failed: {0}")]
    ResponseParsing(String),
    
    /// Request preparation failed
    #[error("Request preparation failed: {0}")]
    RequestPreparation(String),
    
    /// Other errors
    #[error("{0}")]
    Others(String),
}

/// Errors that can occur during attestation
#[derive(Error, Debug)]
pub enum AttestationError {
    /// Attestation verification failed
    #[error("Attestation verification failed: {0}")]
    VerificationFailed(String),
    
    /// Invalid attestation data
    #[error("Invalid attestation data: {0}")]
    InvalidData(String),
    
    /// Cryptographic error
    #[error("Cryptographic error: {0}")]
    Cryptographic(String),
}

/// Result type for Aristo client operations
pub type Result<T> = std::result::Result<T, ClientError>;
