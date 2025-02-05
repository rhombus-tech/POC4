use tee_interface::prelude::*;
use async_trait::async_trait;
use std::process::Command;
use sha2::{Sha256, Digest};
use wasmlanche::simulator::Simulator;
use std::sync::Arc;
use tokio::sync::RwLock;
use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize)]
struct WasmInput {
    data: Vec<u8>,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct WasmOutput {
    data: Vec<u8>,
}

pub struct EnarxController {
    config: TeeConfig,
    tee_type: TeeType,
    attestations: Arc<RwLock<Vec<TeeAttestation>>>,
    worker_ids: Vec<String>,
    max_tasks: u32,
    config_path: String,
    simulator: Arc<RwLock<Simulator>>,
}

impl EnarxController {
    pub fn new(tee_type: TeeType, config_path: impl Into<String>) -> Self {
        Self {
            config: TeeConfig::default(),
            tee_type,
            attestations: Arc::new(RwLock::new(Vec::new())),
            worker_ids: vec!["worker-1".to_string()], // TODO: Get real worker IDs
            max_tasks: 10, // TODO: Get from config
            config_path: config_path.into(),
            simulator: Arc::new(RwLock::new(Simulator::new())),
        }
    }

    fn check_enarx_installed() -> bool {
        Command::new("enarx")
            .arg("--version")
            .output()
            .is_ok()
    }

    fn check_platform_support(&self) -> bool {
        // Check if the platform supports the required TEE type
        match self.tee_type {
            TeeType::SGX => {
                Command::new("enarx")
                    .arg("platform")
                    .arg("info")
                    .output()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .contains("sgx")
                    })
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    fn generate_attestation(&self, output: &[u8]) -> TeeAttestation {
        // Generate a mock attestation for now
        let mut hasher = Sha256::new();
        hasher.update(output);
        let hash = hasher.finalize();

        TeeAttestation {
            enclave_id: hash[..].try_into().unwrap_or([0u8; 32]),
            measurement: vec![],
            data: output.to_vec(),
            signature: vec![],
            region_proof: None,
        }
    }

    async fn execute_with_params(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Get wasmlanche simulator
        let mut simulator = self.simulator.write().await;

        // Create contract from WASM bytes
        let contract_result = simulator.create_contract("memory")
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?;

        // Prepare input
        let input = WasmInput {
            data: payload.input.clone(),
        };

        // Serialize input
        let input_bytes = borsh::to_vec(&input)
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?;

        // Execute contract
        let output_bytes = simulator
            .call_contract::<Vec<u8>, Vec<u8>>(contract_result.address, "execute", input_bytes, 1_000_000)
            .map_err(|e| TeeError::ExecutionError(format!("Contract execution failed: {}", e)))?;

        // Generate attestation
        let attestation = self.generate_attestation(&output_bytes);

        Ok(ExecutionResult {
            result: output_bytes,
            state_hash: vec![0u8; 32], // Mock state hash
            stats: ExecutionStats {
                execution_time: std::time::Duration::from_secs(1).as_millis() as u64,
                memory_used: 1024, // Mock memory usage
                syscall_count: 5,  // Mock syscall count
            },
            attestation,
        })
    }

    fn get_region_info(&self) -> Result<RegionInfo, TeeError> {
        Ok(RegionInfo {
            worker_ids: vec!["enarx-worker-1".to_string()],
            max_tasks: 10,
        })
    }

    pub async fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(self.attestations.read().await.clone())
    }
}

#[async_trait]
impl TeeController for EnarxController {
    async fn init(&mut self) -> Result<(), TeeError> {
        // Check if enarx is installed
        #[cfg(not(test))]
        {
            let output = Command::new("which")
                .arg("enarx")
                .output()
                .map_err(|e| TeeError::StateError(format!("enarx not found in PATH: {}", e)))?;
            if !output.status.success() {
                return Err(TeeError::StateError("enarx not found in PATH".to_string()));
            }
        }

        // Check if platform supports required TEE type
        #[cfg(not(test))]
        {
            match self.tee_type {
                TeeType::SGX => {
                    if !self.check_platform_support() {
                        return Err(TeeError::StateError("Platform does not support SGX".to_string()));
                    }
                }
                _ => return Err(TeeError::StateError("Unsupported TEE type".to_string())),
            }
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
    use std::fs;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_wasmlanche_execution() {
        // Create a mock controller that always returns true for platform checks
        let mut controller = EnarxController {
            tee_type: TeeType::SGX,
            config: TeeConfig::default(),
            worker_ids: vec![],
            max_tasks: 1,
            config_path: "config.json".to_string(),
            simulator: Arc::new(RwLock::new(Simulator::new())),
            attestations: Arc::new(RwLock::new(vec![])),
        };

        // Get the path to the wasm module
        let mut wasm_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        wasm_path.push("../wasm-module/target/wasm32-unknown-unknown/release/wasm_module.wasm");

        // Read the wasm module
        let wasm_bytes = fs::read(&wasm_path).expect("Failed to read WASM module");

        let payload = ExecutionPayload {
            execution_id: 1,
            input: wasm_bytes,
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        // Initialize the controller first
        controller.init().await.unwrap();

        // Use the debug build of wasm_module
        let result = controller.execute(&payload).await.unwrap();
        assert!(!result.result.is_empty());
        assert!(result.attestation.measurement.is_empty());
    }
}