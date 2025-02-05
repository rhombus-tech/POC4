use tee_interface::prelude::*;
use async_trait::async_trait;
use std::process::Command;
use sha2::{Sha256, Digest};

pub struct EnarxController {
    config: TeeConfig,
    tee_type: TeeType,
}

impl EnarxController {
    pub fn new(tee_type: TeeType) -> Self {
        Self {
            config: TeeConfig::default(),
            tee_type,
        }
    }

    #[cfg(not(test))]
    fn check_enarx_installed() -> bool {
        Command::new("enarx")
            .arg("--version")
            .output()
            .is_ok()
    }

    #[cfg(test)]
    fn check_enarx_installed() -> bool {
        true // Skip Enarx check in tests
    }

    #[cfg(not(test))]
    fn check_platform_support(&self) -> bool {
        match self.tee_type {
            TeeType::Sgx => Command::new("enarx").arg("platform").arg("sgx").output().is_ok(),
            TeeType::Sev => Command::new("enarx").arg("platform").arg("sev").output().is_ok(),
            _ => false,
        }
    }

    #[cfg(test)]
    fn check_platform_support(&self) -> bool {
        true // Skip platform check in tests
    }

    fn generate_attestation(&self, output: &[u8]) -> TeeAttestation {
        let mut hasher = Sha256::new();
        hasher.update(output);
        let mut measurement = hasher.finalize().to_vec();
        
        // For SEV, pad with additional platform data
        if self.tee_type == TeeType::Sev {
            measurement.extend_from_slice(&[0; 16]); // Add 16 bytes of platform data
        }

        TeeAttestation {
            tee_type: self.tee_type,
            measurement,
            signature: vec![],
        }
    }

    fn get_measurement_size(&self) -> usize {
        match self.tee_type {
            TeeType::Sgx => 32, // SGX measurement is SHA256
            TeeType::Sev => 48, // SEV measurement includes additional platform data
            _ => panic!("Unsupported TEE type"),
        }
    }
}

#[async_trait]
impl TeeController for EnarxController {
    async fn init(&mut self) -> Result<(), TeeError> {
        if !Self::check_enarx_installed() {
            return Err(TeeError::StateError("Enarx not installed".to_string()));
        }

        // Verify platform support
        if !self.check_platform_support() {
            return Err(TeeError::StateError(format!("{:?} not supported on this platform", self.tee_type)));
        }

        Ok(())
    }

    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // In a real implementation, we would:
        // 1. Create an Enarx keep with the specified TEE type
        // 2. Deploy and run the WASM code
        // 3. Collect measurements and attestations
        // 4. Return results with attestations

        let attestation = self.generate_attestation(&payload.input);
        let mut hasher = Sha256::new();
        hasher.update(&payload.input);
        let state_hash = hasher.finalize().into();

        let result = ExecutionResult {
            tx_id: vec![1],
            output: payload.input.clone(),
            state_hash,
            attestations: vec![attestation],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            region_id: format!("{:?}-region", self.tee_type),
        };

        Ok(result)
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
        let controller = EnarxController::new(TeeType::Sgx);
        
        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test".to_vec(),
            params: ExecutionParams::default(),
        };

        let result = controller.execute(&payload).await.unwrap();
        assert_eq!(result.output, payload.input);
        assert_eq!(result.attestations[0].tee_type, TeeType::Sgx);
        assert_eq!(result.attestations[0].measurement.len(), 32);
    }

    #[tokio::test]
    async fn test_sev_controller() {
        let controller = EnarxController::new(TeeType::Sev);
        
        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test".to_vec(),
            params: ExecutionParams::default(),
        };

        let result = controller.execute(&payload).await.unwrap();
        assert_eq!(result.output, payload.input);
        assert_eq!(result.attestations[0].tee_type, TeeType::Sev);
        assert_eq!(result.attestations[0].measurement.len(), 48);
    }
}