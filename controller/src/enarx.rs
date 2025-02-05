use tee_interface::prelude::*;
use async_trait::async_trait;
use std::process::Command;
use sha2::{Sha256, Digest};
use wasmlanche::simulator::{Simulator, SimpleState};
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
    attestations: Vec<TeeAttestation>,
    worker_ids: Vec<String>,
    max_tasks: u32,
    config_path: String,
    state: Arc<RwLock<SimpleState>>,
}

impl EnarxController {
    pub fn new(tee_type: TeeType, config_path: impl Into<String>) -> Self {
        // Initialize wasmlanche state
        let state = SimpleState::default();

        Self {
            config: TeeConfig::default(),
            tee_type,
            attestations: Vec::new(),
            worker_ids: vec!["worker-1".to_string()], // TODO: Get real worker IDs
            max_tasks: 10, // TODO: Get from config
            config_path: config_path.into(),
            state: Arc::new(RwLock::new(state)),
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
        let mut hasher = Sha256::new();
        hasher.update(output);
        let measurement = hasher.finalize().to_vec();

        TeeAttestation {
            enclave_id: [0u8; 32], // TODO: Get real enclave ID
            measurement,
            data: output.to_vec(),
            signature: vec![0u8; 64], // TODO: Generate real signature
            region_proof: Some(vec![0u8; 32]), // TODO: Generate real proof
        }
    }

    async fn execute_with_params(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Get wasmlanche state
        let mut state = self.state.write().await;
        let simulator = Simulator::new(&mut state);

        // Create contract from WASM bytes
        let contract_result = simulator.create_contract("memory")
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?;

        // Prepare input
        let input = WasmInput {
            data: payload.input.clone(),
        };

        // Execute the contract
        let result: WasmOutput = simulator.call_contract(
            contract_result.address,
            "execute",
            &input,
            1_000_000, // Gas limit
        ).map_err(|e| TeeError::ExecutionError(e.to_string()))?;

        // Generate attestation for the result
        let attestation = self.generate_attestation(&result.data);

        Ok(ExecutionResult {
            result: result.data,
            attestation,
            state_hash: vec![0u8; 32], // TODO: Get real state hash
            stats: ExecutionStats {
                execution_time: 100, // TODO: Get real stats
                memory_used: 1024,
                syscall_count: 5,
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
            return Err(TeeError::ExecutionError("Enarx not installed".into()));
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
    async fn test_wasmlanche_execution() {
        let controller = EnarxController::new(TeeType::SGX, "test_config.json");
        
        // Create a simple test payload
        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test input".to_vec(),
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        let result = controller.execute(&payload).await.unwrap();
        assert!(!result.result.is_empty());
        assert!(!result.state_hash.is_empty());
    }
}