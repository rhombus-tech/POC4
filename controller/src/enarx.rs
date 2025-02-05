use tee_interface::prelude::*;
use async_trait::async_trait;
use std::process::Command;
use sha2::{Sha256, Digest};

pub struct EnarxController {
    config: TeeConfig,
    tee_type: TeeType,
    attestations: Vec<TeeAttestation>,
    worker_ids: Vec<String>,
    max_tasks: u32,
    config_path: String,
}

impl EnarxController {
    pub fn new(tee_type: TeeType, config_path: impl Into<String>) -> Self {
        Self {
            config: TeeConfig::default(),
            tee_type,
            attestations: Vec::new(),
            worker_ids: vec!["worker-1".to_string()], // TODO: Get real worker IDs
            max_tasks: 10, // TODO: Get from config
            config_path: config_path.into(),
        }
    }

    #[cfg(not(test))]
    fn check_enarx_installed() -> bool {
        Command::new("enarx")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[cfg(test)]
    fn check_enarx_installed() -> bool {
        true
    }

    fn check_platform_support(&self) -> bool {
        match self.tee_type {
            TeeType::SGX => Command::new("enarx")
                .args(["platform", "info"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("sgx"))
                .unwrap_or(false),
            TeeType::SEV => Command::new("enarx")
                .args(["platform", "info"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("sev"))
                .unwrap_or(false),
        }
    }

    fn generate_attestation(&self, output: &[u8]) -> TeeAttestation {
        // Generate a mock attestation for testing
        let mut hasher = Sha256::new();
        hasher.update(output);
        let measurement = hasher.finalize().to_vec();

        TeeAttestation {
            enclave_id: [1u8; 32],
            measurement,
            data: b"test attestation".to_vec(),
            signature: vec![2u8; 64],
            region_proof: Some(vec![3u8; 32]),
        }
    }

    async fn execute_with_params(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // TODO: Implement real execution
        let result = b"test output".to_vec();
        let attestation = self.generate_attestation(&result);

        Ok(ExecutionResult {
            result,
            attestation,
            state_hash: vec![4u8; 32],
            stats: ExecutionStats {
                execution_time: 1000,
                memory_used: 1024,
                syscall_count: 10,
            },
        })
    }

    pub fn get_region_info(&self) -> Result<RegionInfo, TeeError> {
        Ok(RegionInfo {
            worker_ids: self.worker_ids.clone(),
            max_tasks: self.max_tasks,
        })
    }

    pub async fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(self.attestations.clone())
    }
}

#[async_trait]
impl TeeController for EnarxController {
    async fn init(&mut self) -> Result<(), TeeError> {
        if !Self::check_enarx_installed() {
            return Err(TeeError::ExecutionError("Enarx not installed".to_string()));
        }

        if !self.check_platform_support() {
            return Err(TeeError::ExecutionError(format!(
                "{:?} not supported on this platform",
                self.tee_type
            )));
        }

        Ok(())
    }

    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        self.execute_with_params(payload).await
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, new_config: TeeConfig) -> Result<(), TeeError> {
        self.config = new_config;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sgx_controller() {
        let mut controller = EnarxController::new(TeeType::SGX, "path/to/config");
        assert!(controller.init().await.is_ok());

        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test input".to_vec(),
            params: ExecutionParams::default(),
        };

        let result = controller.execute(&payload).await.unwrap();
        assert!(!result.result.is_empty());
        assert!(!result.attestation.measurement.is_empty());
    }

    #[tokio::test]
    async fn test_sev_controller() {
        let mut controller = EnarxController::new(TeeType::SEV, "path/to/config");
        assert!(controller.init().await.is_ok());

        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test input".to_vec(),
            params: ExecutionParams::default(),
        };

        let result = controller.execute(&payload).await.unwrap();
        assert!(!result.result.is_empty());
        assert!(!result.attestation.measurement.is_empty());
    }
}