/*!
 * Protocol types for cross-regional communication
 * 
 * This module includes both public API types and internal binary protocol types
 * optimized for efficient TEE-agnostic communication supporting both Intel SGX
 * and AMD SEV environments.
 */

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Supported parameter formats for WebAssembly contracts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParameterFormat {
    /// Length-prefixed format (4-byte length + data)
    LengthPrefixed,
    
    /// Direct parameters (raw data without length prefix)
    Direct,
    
    /// Empty parameters (all zeros)
    Empty,
}

/// Transaction request for cross-regional execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRequest {
    /// ID of the transaction
    pub id: String,
    
    /// Source region identifier
    pub source_region: String,
    
    /// Target region identifier
    pub target_region: String,
    
    /// Contract identifier
    pub contract_id: String,
    
    /// Function to call
    pub function: String,
    
    /// Parameters to pass to the function
    pub parameters: Vec<u8>,
    
    /// Parameter format to use
    pub parameter_format: ParameterFormat,
    
    /// Timestamp of the request
    pub timestamp: u64,
    
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Transaction response from cross-regional execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    /// ID of the transaction (matches request)
    pub id: String,
    
    /// Source region identifier
    pub source_region: String,
    
    /// Target region identifier
    pub target_region: String,
    
    /// Execution result
    pub result: ExecutionResult,
    
    /// Raw result data
    pub data: Vec<u8>,
    
    /// Attestation data for verification
    pub attestation: Option<AttestationData>,
    
    /// Timestamp of the response
    pub timestamp: u64,
    
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Result of transaction execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionResult {
    /// Execution succeeded
    Success,
    
    /// Execution failed
    Failure,
    
    /// Execution is pending
    Pending,
    
    /// Execution was rejected
    Rejected,
    
    /// Unknown execution result
    Unknown,
}

/// Attestation data for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationData {
    /// TEE identity
    pub tee_identity: String,
    
    /// Attestation report
    pub report: Vec<u8>,
    
    /// Signature over the transaction and result
    pub signature: Vec<u8>,
    
    /// Public key for signature verification
    pub public_key: Vec<u8>,
}

/// Cross-regional verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    /// Transaction ID to verify
    pub transaction_id: String,
    
    /// List of regions to include in verification
    pub regions: Vec<String>,
    
    /// Additional context for verification
    pub context: HashMap<String, String>,
}

/// Cross-regional verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResponse {
    /// Transaction ID that was verified
    pub transaction_id: String,
    
    /// Verification result
    pub result: VerificationResult,
    
    /// Per-region verification details
    pub region_results: HashMap<String, RegionVerificationResult>,
    
    /// Timestamp of verification
    pub timestamp: u64,
}

/// Result of cross-regional verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationResult {
    /// Verification succeeded
    Verified,
    
    /// Verification failed
    Failed,
    
    /// Verification is pending
    Pending,
}

/// Per-region verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionVerificationResult {
    /// Verification status for this region
    pub status: VerificationStatus,
    
    /// Detailed information if verification failed
    pub details: Option<String>,
}

/// Status of verification for a region
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Region verified successfully
    Success,
    
    /// Verification failed for this region
    Failed,
    
    /// Region could not be reached
    Unreachable,
    
    /// Region is not trusted
    Untrusted,
    
    /// Accumulator mismatch between expected and actual values
    AccumulatorMismatch,
}

//---------------------------------------------------------------------
// Binary Protocol Message Types
//---------------------------------------------------------------------

/// Binary protocol transaction request
/// 
/// Optimized for efficient serialization in TEE environments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryTransactionRequest {
    /// Transaction ID
    pub transaction_id: String,
    
    /// Source region
    pub source_region: String,
    
    /// Target region
    pub target_region: String,
    
    /// Contract ID
    pub contract_id: String,
    
    /// Function name
    pub function: String,
    
    /// Parameters (already formatted according to parameter_format)
    pub parameters: Vec<u8>,
    
    /// Timestamp
    pub timestamp: u64,
}

/// Binary protocol transaction response
/// 
/// Optimized for efficient serialization in TEE environments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryTransactionResponse {
    /// Status code (0=success, 1=failure, 2=rejected)
    pub status: u8,
    
    /// Result data bytes
    pub result_data: Vec<u8>,
    
    /// Attestation data (optional)
    pub attestation: Option<BinaryAttestationData>,
    
    /// Accumulator value for verification (32 bytes)
    pub accumulator: Option<Vec<u8>>,
    
    /// Timestamp
    pub timestamp: u64,
}

/// Binary protocol attestation data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryAttestationData {
    /// TEE identity
    pub tee_identity: String,
    
    /// Attestation report
    pub report: Vec<u8>,
    
    /// Signature
    pub signature: Vec<u8>,
    
    /// Public key
    pub public_key: Vec<u8>,
}

/// Binary protocol verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryVerificationRequest {
    /// Transaction ID to verify
    pub transaction_id: String,
    
    /// Regions to verify across
    pub regions: Vec<String>,
    
    /// Context data
    pub context: Vec<u8>,
}

/// Binary protocol verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryVerificationResponse {
    /// Whether the transaction is verified
    pub verified: bool,
    
    /// Verification details
    pub details: Option<String>,
    
    /// Current accumulator value
    pub accumulator: Option<Vec<u8>>,
    
    /// Timestamp
    pub timestamp: u64,
}
