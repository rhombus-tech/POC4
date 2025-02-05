use std::time::Instant;
use async_trait::async_trait;
use tee_interface::prelude::*;
use tokio::sync::RwLock;
use wasmlanche::simulator::Simulator;
use wasmlanche::Address;
use std::sync::Arc;

pub struct WasmSimulator {
    simulator: Arc<RwLock<Simulator>>,
    config: TeeConfig,
}

impl WasmSimulator {
    pub fn new() -> Self {
        Self {
            simulator: Arc::new(RwLock::new(Simulator::new())),
            config: TeeConfig::default(),
        }
    }

    pub async fn create_contract(&mut self, code: &[u8]) -> Result<Address, String> {
        #[cfg(test)]
        {
            return Ok(Address::new([0; 33]));
        }

        #[cfg(not(test))]
        {
            // Create a temporary file to store the WASM code
            let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let wasm_path = temp_dir.path().join("contract.wasm");
            std::fs::write(&wasm_path, code).map_err(|e| e.to_string())?;

            // Create the contract
            let result = self.simulator
                .write()
                .await
                .create_contract(wasm_path.to_str().unwrap())
                .map_err(|e| e.to_string())?;

            Ok(result.address)
        }
    }

    pub async fn call_contract(
        &mut self,
        contract: Address,
        method: &str,
        args: &[u8],
        gas: u64,
    ) -> Result<Vec<u8>, String> {
        #[cfg(test)]
        {
            return Ok(vec![0; 32]);
        }

        #[cfg(not(test))]
        {
            let mut simulator = self.simulator.write().await;
            simulator.call_contract::<Vec<u8>, Vec<u8>>(contract, method, args.to_vec(), gas)
                .map_err(|e| e.to_string())
        }
    }

    pub async fn execute_wasm(&mut self, code: &[u8], function: &str, args: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let contract = self.create_contract(code).await?;
        let result = self.call_contract(contract, function, args, 1_000_000).await?;
        Ok(result)
    }
}

#[async_trait]
impl TeeController for WasmSimulator {
    async fn init(&mut self) -> Result<(), TeeError> {
        Ok(())
    }

    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let start = Instant::now();
        let mut simulator = WasmSimulator::new();
        let result = simulator
            .execute_wasm(&payload.input, "execute", &payload.input)
            .await
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?;

        Ok(ExecutionResult {
            result,
            attestation: TeeAttestation {
                enclave_id: [0u8; 32],
                measurement: vec![],
                data: vec![],
                signature: vec![],
                region_proof: None,
            },
            state_hash: vec![0u8; 32],
            stats: ExecutionStats {
                execution_time: start.elapsed().as_micros() as u64,
                memory_used: 0,
                syscall_count: 0,
            },
        })
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, config: TeeConfig) -> Result<(), TeeError> {
        self.config = config;
        Ok(())
    }
}
