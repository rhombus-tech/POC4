use std::path::Path;
use thiserror::Error;
use tee_interface::{self, TeeController, TeeError};
use tee_interface::types::{
    ExecutionInput, ExecutionResult, ExecutionPayload, ExecutionParams,
    TeeConfig
};
use borsh::{BorshSerialize, BorshDeserialize};
use rand::random;
use tempfile::tempdir;
use async_trait::async_trait;
use std::io;

/// Errors that can occur during Enarx operations
#[derive(Error, Debug)]
pub enum ControllerError {
    #[error("Failed to initialize Enarx: {0}")]
    InitializationError(String),

    #[error("Failed to execute in Enarx: {0}")]
    ExecutionError(String),

    #[error("Failed to serialize/deserialize: {0}")]
    SerializationError(String),

    #[error("IO error: {0}")]
    IoError(io::Error),

    #[error("Failed to verify attestation: {0}")]
    AttestationError(String),

    #[error("Platform error: {0}")]
    PlatformError(String),
}

impl From<io::Error> for ControllerError {
    fn from(err: io::Error) -> Self {
        ControllerError::IoError(err)
    }
}

/// Controller for Enarx TEE execution
#[derive(Default)]
pub struct EnarxController {
    // Configuration for Enarx
    config: TeeConfig,
}

impl EnarxController {
    /// Create a new Enarx controller
    pub fn new(config: TeeConfig) -> Self {
        Self { config }
    }

    /// Check if Enarx is available and properly configured
    pub async fn check_enarx() -> Result<bool, ControllerError> {
        // Check Enarx binary
        let enarx_check = std::process::Command::new("enarx")
            .arg("--version")
            .output()
            .map_err(|e| ControllerError::InitializationError(format!("Enarx not found: {}", e)))?;

        if !enarx_check.status.success() {
            return Ok(false);
        }

        // Check if we can create a basic payload
        let test_payload = ExecutionPayload {
            execution_id: 0,
            input: vec![],
            params: ExecutionParams::default(),
        };

        let payload_bytes = test_payload.try_to_vec()
            .map_err(|e| ControllerError::SerializationError(format!("Failed to serialize test payload: {}", e)))?;

        // Create temporary directory for test
        let temp_dir = tempdir()
            .map_err(|e| ControllerError::InitializationError(format!("Failed to create temp dir: {}", e)))?;

        let test_path = temp_dir.path().join("test.bin");
        std::fs::write(&test_path, &payload_bytes)?;

        Ok(true)
    }

    /// Verify attestation from Enarx
    async fn verify_attestation(&self, attestation: &[u8]) -> Result<(), ControllerError> {
        // TODO: Implement actual attestation verification
        if attestation.is_empty() {
            return Err(ControllerError::AttestationError("Empty attestation".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl TeeController for EnarxController {
    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionResult, TeeError> {
        // Convert input to payload
        let input_bytes = input.wasm_bytes.clone();

        // Create execution payload with config parameters
        let payload = ExecutionPayload {
            execution_id: random(),
            input: input_bytes,
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false, // No debug mode in config
            },
        };

        // Serialize payload
        let payload_bytes = payload.try_to_vec()
            .map_err(|e| TeeError::ExecutionError(format!("Failed to serialize payload: {}", e)))?;

        // Create temporary directory for execution
        let temp_dir = tempdir()
            .map_err(|e| TeeError::ExecutionError(format!("Failed to create temp dir: {}", e)))?;

        let payload_path = temp_dir.path().join("payload.bin");
        std::fs::write(&payload_path, &payload_bytes)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to write payload: {}", e)))?;

        // Execute in Enarx with config parameters
        let mut command = std::process::Command::new("enarx");
        command.arg("run")
            .arg("--wasmcfgfile")
            .arg(&payload_path);

        // Add memory size if configured
        if self.config.memory_size > 0 {
            command.arg("--memory-size")
                .arg(self.config.memory_size.to_string());
        }

        // Add CPU cores if configured
        if self.config.num_cores > 0 {
            command.arg("--cpus")
                .arg(self.config.num_cores.to_string());
        }

        let output = command.output()
            .map_err(|e| TeeError::ExecutionError(format!("Failed to execute in Enarx: {}", e)))?;

        if !output.status.success() {
            return Err(TeeError::ExecutionError(format!(
                "Enarx execution failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Verify attestation
        self.verify_attestation(&output.stdout)
            .await
            .map_err(|e| TeeError::AttestationError(e.to_string()))?;

        // Parse result
        let result: ExecutionResult = BorshDeserialize::try_from_slice(&output.stdout)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to deserialize result: {}", e)))?;

        Ok(result)
    }

    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error>> {
        // Check Enarx installation
        let enarx_check = std::process::Command::new("enarx")
            .arg("--version")
            .output()?;

        let enarx_ok = enarx_check.status.success();

        // Check wasmldr
        let wasmldr_check = std::process::Command::new("wasmldr")
            .arg("--version")
            .output()?;

        let wasmldr_ok = wasmldr_check.status.success();

        Ok(enarx_ok && wasmldr_ok)
    }
}

pub async fn execute_sgx(
    wasm_path: &Path,
) -> Result<ExecutionResult, ControllerError> {
    let _controller = EnarxController::new(TeeConfig::default());
    let wasm_bytes = std::fs::read(wasm_path)?;
    let input = ExecutionInput {
        wasm_bytes,
        function: "main".to_string(),
        args: vec![],  // Empty args for now
    };
    _controller.execute(input).await.map_err(|e| ControllerError::ExecutionError(e.to_string()))
}

pub async fn execute_sev(
    wasm_path: &Path,
) -> Result<ExecutionResult, ControllerError> {
    let _controller = EnarxController::new(TeeConfig::default());
    let wasm_bytes = std::fs::read(wasm_path)?;
    let input = ExecutionInput {
        wasm_bytes,
        function: "main".to_string(),
        args: vec![],  // Empty args for now
    };
    _controller.execute(input).await.map_err(|e| ControllerError::ExecutionError(e.to_string()))
}

pub fn verify_platforms() -> Result<(bool, bool), ControllerError> {
    // Check SGX support
    let sgx_supported = std::process::Command::new("enarx")
        .args(["platform", "info", "--sgx"])
        .output()
        .map_err(|e| ControllerError::PlatformError(e.to_string()))?
        .status
        .success();

    // Check SEV support
    let sev_supported = std::process::Command::new("enarx")
        .args(["platform", "info", "--sev"])
        .output()
        .map_err(|e| ControllerError::PlatformError(e.to_string()))?
        .status
        .success();

    Ok((sgx_supported, sev_supported))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_controller_initialization() {
        let _controller = EnarxController::new(TeeConfig::default());
        assert!(EnarxController::check_enarx().await.is_ok());
    }

    #[tokio::test]
    async fn test_platform_verification() {
        let (sgx, sev) = verify_platforms().unwrap();
        println!("SGX supported: {}", sgx);
        println!("SEV supported: {}", sev);
    }

    #[tokio::test]
    async fn test_tee_execution() {
        let wasm_path = Path::new("test.wasm");
        
        // Test execution (this will fail without actual WASM module)
        let result = execute_sgx(wasm_path).await;
        assert!(result.is_err()); // Should fail because we didn't provide real WASM
    }
}