use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeError, TeeAttestation, Region, ExecutionResult, ExecutionStats, TeeType};
use wasmlanche::{
    simulator::Simulator,
    types::Address as WasmlAddress,
};
use uuid::Uuid;
use chrono;
use sha2::{Sha256, Digest};

const DEFAULT_GAS: u64 = 1_000_000;

pub struct SimulatorController {
    simulator: Arc<RwLock<Simulator>>,
    contracts: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl SimulatorController {
    pub fn new() -> Self {
        let default_address = WasmlAddress::new([0; 33]); // Create a default address for the simulator
        Self {
            simulator: Arc::new(RwLock::new(Simulator::new(default_address.into()))),
            contracts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn create_deterministic_address(contract_bytes: &[u8]) -> WasmlAddress {
        let mut hasher = Sha256::new();
        hasher.update(contract_bytes);
        let result = hasher.finalize();
        let mut addr = [0u8; 33];
        addr[..32].copy_from_slice(&result);
        addr[32] = 0; // Version byte
        WasmlAddress::new(addr)
    }
}

#[async_trait::async_trait]
impl TeeExecutor for SimulatorController {
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Get contract code first
        let contract_code = {
            let contracts = self.contracts.read().await;
            contracts
                .get(&payload.params.id_to)
                .ok_or_else(|| TeeError::Contract("Contract not found".to_string()))?
                .clone()
        };

        // Execute contract
        let result = {
            let mut simulator = self.simulator.write().await;
            let execution = simulator.execute(
                &contract_code,
                &payload.params.function_call,
                &payload.input,
                DEFAULT_GAS,
            ).await.map_err(|e| TeeError::Contract(e.to_string()))?;
            execution
        };

        Ok(ExecutionResult {
            result,
            state_hash: vec![0; 32], // Placeholder state hash
            stats: ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
            },
            attestations: vec![TeeAttestation {
                enclave_id: b"simulator".to_vec(),
                measurement: vec![0; 32],
                timestamp: chrono::Utc::now().timestamp() as u64,
                data: vec![],
                signature: vec![],
                region_proof: None,
                enclave_type: TeeType::SGX,
            }],
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn deploy_contract(&self, wasm_code: &[u8], _region_id: &str) -> Result<String, TeeError> {
        // Generate contract ID
        let contract_id = Uuid::new_v4().to_string();

        // Store contract code
        {
            let mut contracts = self.contracts.write().await;
            contracts.insert(contract_id.clone(), wasm_code.to_vec());
        }

        // Deploy contract
        {
            let mut simulator = self.simulator.write().await;
            let execution = simulator.execute(wasm_code, "deploy", &[], DEFAULT_GAS)
                .await.map_err(|e| TeeError::Contract(e.to_string()))?;
        }

        Ok(contract_id)
    }

    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        let contracts = self.contracts.read().await;
        if let Some(code) = contracts.get(contract_address) {
            let mut hasher = Sha256::new();
            hasher.update(code);
            Ok(hasher.finalize().to_vec())
        } else {
            Err(TeeError::Contract("Contract not found".to_string()))
        }
    }

    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
        Ok(vec![Region {
            id: "simulator".to_string(),
            worker_ids: vec!["simulator-1".to_string()],
            max_tasks: 100,
        }])
    }

    async fn get_attestations(&self, _region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(vec![TeeAttestation {
            enclave_id: b"simulator".to_vec(),
            measurement: vec![0; 32],
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: vec![],
            signature: vec![],
            region_proof: None,
            enclave_type: TeeType::SGX,
        }])
    }
}
