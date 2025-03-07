use std::fmt;
use std::error::Error;
use tee_interface::TeeError;

/// Error types for Enarx operations
#[derive(Debug)]
pub enum EnarxError {
    /// Error with the Enarx Keep Manager
    KeepManagerError(String),
    /// Error with contract execution
    ExecutionError(String),
    /// Error with contract deployment
    DeploymentError(String),
    /// Error with contract state access
    StateError(String),
    /// Error with I/O operations
    IoError(std::io::Error),
    /// Other error types
    Other(String),
}

impl fmt::Display for EnarxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeepManagerError(msg) => write!(f, "Keep Manager error: {}", msg),
            Self::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            Self::DeploymentError(msg) => write!(f, "Deployment error: {}", msg),
            Self::StateError(msg) => write!(f, "State error: {}", msg),
            Self::IoError(e) => write!(f, "I/O error: {}", e),
            Self::Other(msg) => write!(f, "Other error: {}", msg),
        }
    }
}

impl Error for EnarxError {}

impl From<std::io::Error> for EnarxError {
    fn from(error: std::io::Error) -> Self {
        EnarxError::IoError(error)
    }
}

impl From<EnarxError> for TeeError {
    fn from(error: EnarxError) -> Self {
        match error {
            EnarxError::KeepManagerError(msg) => TeeError::ExecutionError(format!("Keep Manager: {}", msg)),
            EnarxError::ExecutionError(msg) => TeeError::ExecutionError(msg),
            EnarxError::DeploymentError(msg) => TeeError::Contract(msg),
            EnarxError::StateError(msg) => TeeError::Contract(msg),
            EnarxError::IoError(e) => TeeError::ExecutionError(format!("I/O error: {}", e)),
            EnarxError::Other(msg) => TeeError::ExecutionError(msg),
        }
    }
}
