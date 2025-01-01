use std::path::Path;
use std::process::Command;
use tee_interface::prelude::*;
use thiserror::Error;

pub struct Controller {
    verbose: bool,
}

#[derive(Error, Debug)]
pub enum ControllerError {
    #[error("Enarx execution failed: {0}")]
    EnarxError(String),
    
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] std::io::Error),
    
    #[error("Platform error: {0}")]
    PlatformError(String),
}

impl Controller {
    pub fn new(verbose: bool) -> Result<Self, ControllerError> {
        // Check if Enarx is installed
        let enarx_version = Command::new("enarx")
            .arg("--version")
            .output()
            .map_err(|_| ControllerError::PlatformError("Enarx not found".into()))?;

        if !enarx_version.status.success() {
            return Err(ControllerError::PlatformError("Failed to verify Enarx installation".into()));
        }

        Ok(Self { verbose })
    }

    pub async fn execute_sgx(
        &self,
        wasm_path: &Path,
        input_path: &Path,
    ) -> Result<ExecutionResult, ControllerError> {
        self.execute_in_tee(wasm_path, input_path, "sgx").await
    }

    pub async fn execute_sev(
        &self,
        wasm_path: &Path,
        input_path: &Path,
    ) -> Result<ExecutionResult, ControllerError> {
        self.execute_in_tee(wasm_path, input_path, "sev").await
    }

    async fn execute_in_tee(
        &self,
        wasm_path: &Path,
        input_path: &Path,
        backend: &str,
    ) -> Result<ExecutionResult, ControllerError> {
        // Read input file
        let input_data = std::fs::read(input_path)?;

        // Prepare payload
        let payload = ExecutionPayload {
            execution_id: rand::random(),
            input: input_data,
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: self.verbose,
            },
        };

        // Serialize payload
        let payload_bytes = borsh::to_vec(&payload)
            .map_err(|e| ControllerError::SerializationError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )))?;

        // Create temporary file for payload
        let temp_dir = tempfile::tempdir()?;
        let payload_path = temp_dir.path().join("payload.bin");
        std::fs::write(&payload_path, &payload_bytes)?;

        // Execute in Enarx
        let output = Command::new("enarx")
            .args([
                "run",
                "--backend",
                backend,
                wasm_path.to_str().unwrap(),
                "--",
                payload_path.to_str().unwrap(),
            ])
            .output()?;

        if !output.status.success() {
            return Err(ControllerError::EnarxError(
                String::from_utf8_lossy(&output.stderr).into_owned()
            ));
        }

        // Deserialize result
        let result: ExecutionResult = borsh::from_slice(&output.stdout)
            .map_err(|e| ControllerError::SerializationError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )))?;

        if self.verbose {
            println!("Execution completed in {} backend:", backend);
            println!("  Result hash: {:?}", result.result_hash);
            println!("  Attestation measurement: {:?}", result.attestation.measurement);
        }

        Ok(result)
    }

    pub fn verify_platforms(&self) -> Result<(bool, bool), ControllerError> {
        // Check SGX support
        let sgx_supported = Command::new("enarx")
            .args(["platform", "info", "--sgx"])
            .output()?
            .status
            .success();

        // Check SEV support
        let sev_supported = Command::new("enarx")
            .args(["platform", "info", "--sev"])
            .output()?
            .status
            .success();

        Ok((sgx_supported, sev_supported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_controller_initialization() {
        let controller = Controller::new(false);
        assert!(controller.is_ok());
    }

    #[tokio::test]
    async fn test_platform_verification() {
        let controller = Controller::new(false).unwrap();
        let (sgx, sev) = controller.verify_platforms().unwrap();
        println!("SGX supported: {}", sgx);
        println!("SEV supported: {}", sev);
    }

    #[tokio::test]
    async fn test_tee_execution() {
        let controller = Controller::new(false).unwrap();
        
        // Create test files
        let temp_dir = tempfile::tempdir().unwrap();
        let wasm_path = temp_dir.path().join("test.wasm");
        let input_path = temp_dir.path().join("input.dat");
        
        // Write test data
        fs::write(&input_path, b"test data").unwrap();
        
        // Test execution (this will fail without actual WASM module)
        let result = controller.execute_sgx(&wasm_path, &input_path).await;
        assert!(result.is_err()); // Should fail because we didn't provide real WASM
    }
}