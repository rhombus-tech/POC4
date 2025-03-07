use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use async_trait::async_trait;
use tee_interface::{TeeExecutor, ExecutionPayload, ExecutionResult, TeeError, ExecutionStats, TeeAttestation, TeeType, Region};
use crate::simulator::Simulator;
use wasmlanche::types::WasmlAddress;
use uuid::Uuid;
use chrono;
use sha2::{Sha256, Digest};
use hex;

// Define a constant for default gas limits
const DEFAULT_GAS: u64 = 1_000_000;

#[derive(Clone)]
struct AsyncOperationState {
    id: String,
    status: String, // "pending", "completed", "failed"
    result: Option<Vec<u8>>,
    context: Option<Vec<u8>>,
    timestamp: String,
}

pub struct HyperTeeController {
    simulator: Arc<RwLock<Simulator>>,
    contracts: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    operations: Arc<RwLock<HashMap<String, AsyncOperationState>>>,
}

impl HyperTeeController {
    pub async fn new() -> Self {
        Self {
            simulator: Arc::new(RwLock::new(Simulator::new().await)),
            contracts: Arc::new(RwLock::new(HashMap::new())),
            operations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Convert an Address to a WasmlAddress
    fn to_wasml_address(address: &WasmlAddress) -> WasmlAddress {
        address.clone()
    }

    // Get balance from simulator
    pub async fn get_balance(&self, account: WasmlAddress) -> Result<u64, TeeError> {
        let simulator_clone = self.simulator.clone();
        
        // Execute in a way that properly handles the mutex guard
        let result = {
            // Use read lock for get_balance since it doesn't modify state
            let simulator = simulator_clone.read().await;
            simulator.get_balance(&account)
        };
        
        Ok(result)
    }
    
    // Set balance in simulator
    pub async fn set_balance(&self, account: WasmlAddress, balance: u64) -> Result<(), TeeError> {
        let simulator_clone = self.simulator.clone();
        
        // Execute in a way that properly handles the mutex guard
        {
            // Use write lock for set_balance since it modifies state
            let mut simulator = simulator_clone.write().await;
            simulator.set_balance(&account, balance);
        }
        
        Ok(())
    }

    pub async fn call_contract<U: AsRef<[u8]>>(
        &mut self,
        contract: WasmlAddress,
        method: &str,
        params: U,
        gas: u64,
    ) -> Result<Vec<u8>, TeeError> {
        let simulator_clone = self.simulator.clone();
        
        // Execute in a way that properly handles the mutex guard
        let result = {
            // Acquire the mutex
            let mut simulator = simulator_clone.write().await;
            // Drop the mutex guard before waiting for async operation
            drop(simulator);
            // Get the simulator again to ensure we don't hold the guard during await
            let mut simulator = simulator_clone.write().await;
            simulator.call_contract(contract, method, params, gas)
                .await
                .map_err(|e| TeeError::Contract(e.to_string()))
        };
        
        result
    }

    pub async fn create_operation(&self, context: Option<Vec<u8>>) -> String {
        let operation_id = Uuid::new_v4().to_string();
        let mut operations = self.operations.write().await;
        operations.insert(operation_id.clone(), AsyncOperationState {
            id: operation_id.clone(),
            status: "pending".to_string(),
            result: None,
            context: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        operation_id
    }

    pub async fn get_operation(&self, operation_id: &str) -> Option<AsyncOperationState> {
        let operations = self.operations.read().await;
        operations.get(operation_id).cloned()
    }

    pub async fn complete_operation(&self, operation_id: &str, result: Vec<u8>) -> Result<(), TeeError> {
        let mut operations = self.operations.write().await;
        if let Some(op) = operations.get_mut(operation_id) {
            op.status = "completed".to_string();
            op.result = Some(result);
            op.timestamp = chrono::Utc::now().to_rfc3339();
            Ok(())
        } else {
            Err(TeeError::Contract(format!("Operation {} not found", operation_id)))
        }
    }

    pub async fn create_contract(&mut self, wasm_code: Vec<u8>) -> Result<WasmlAddress, TeeError> {
        let mut simulator = self.simulator.write().await;
        
        // Drop the mutex guard to avoid holding it during async operation
        drop(simulator);
        
        // Acquire it again for the async call
        let mut simulator = self.simulator.write().await;
        simulator.create_contract(wasm_code)
            .map_err(|e| TeeError::Contract(e.to_string()))
    }

    // Deploy a contract
    pub async fn deploy_contract(&self, wasm_code: &[u8], _region_id: Option<&str>) -> Result<String, TeeError> {
        // Create contract ID using SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(wasm_code);
        let hash = hasher.finalize();
        let contract_id = hex::encode(hash);
        
        // Store contract code
        {
            let mut contracts = self.contracts.write().await;
            contracts.insert(contract_id.clone(), wasm_code.to_vec());
        }
        
        // Deploy contract
        {
            let default_actor = WasmlAddress::new([0; 32]); // Create a default address
            let wasml_actor = Self::to_wasml_address(&default_actor);
            
            // Make a copy of wasm_code since we'll need to drop the guard
            let wasm_code_vec = wasm_code.to_vec();
            let simulator_clone = self.simulator.clone();
            
            // Execute in a way that properly handles the mutex guard
            let result = {
                // Acquire the mutex
                let mut simulator = simulator_clone.write().await;
                // Drop the mutex guard before waiting for async operation
                drop(simulator);
                // Get the simulator again to ensure we don't hold the guard during await
                let mut simulator = simulator_clone.write().await;
                simulator.execute(&wasml_actor, &wasm_code_vec, "init", &[], DEFAULT_GAS)
                    .await
                    .map_err(|e| TeeError::Contract(e.to_string()))
            }?;
        }
        
        Ok(contract_id)
    }

    // Get a deployed contract
    pub async fn get_contract(&self, contract_id: &str) -> Result<Vec<u8>, TeeError> {
        let contracts = self.contracts.read().await;
        contracts.get(contract_id)
            .cloned()
            .ok_or_else(|| TeeError::Contract("Contract not found".to_string()))
    }
}

#[async_trait]
impl TeeExecutor for HyperTeeController {
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Check if this is an async operation query
        if let Some(op_id) = &payload.operation_id {
            // Retrieve operation state
            let operations = self.operations.read().await;
            if let Some(op) = operations.get(op_id) {
                // Create an operation result
                return Ok(ExecutionResult {
                    result: op.result.clone().unwrap_or_default(),
                    state_hash: vec![0; 32], // Mock state hash
                    stats: ExecutionStats {
                        execution_time: 0,
                        memory_used: 0,
                        syscall_count: 0,
                    },
                    attestations: vec![TeeAttestation {
                        enclave_id: b"mock".to_vec(),
                        measurement: vec![0; 32],
                        timestamp: chrono::Utc::now().timestamp() as u64,
                        signature: vec![0; 64],
                        region_proof: Some(vec![0; 32]),
                        data: vec![0; 32],
                        enclave_type: TeeType::SGX,
                    }],
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    operation_status: Some(op.status.clone()),
                    operation_id: Some(op_id.clone()),
                    pending_operations: None,
                });
            }
            
            // Operation not found
            return Err(TeeError::Contract(format!("Operation {} not found", op_id)));
        }
        
        // Check if this is an async operation
        if payload.params.function_call.contains("_async") || 
           payload.previous_operation_id.is_some() {
            
            // Create and store a new operation
            let operation_id_str = Uuid::new_v4().to_string();
            let mut operations = self.operations.write().await;
            operations.insert(operation_id_str.clone(), AsyncOperationState {
                id: operation_id_str.clone(),
                status: "pending".to_string(),
                result: None,
                context: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
            
            // Get clones for async task
            let simulator_clone = self.simulator.clone();
            let operations_clone = self.operations.clone();
            let function_call_clone = payload.params.function_call.clone();
            let input_clone = payload.input.clone();
            let operation_id_clone = operation_id_str.clone();
            
            // Spawn a task to handle the execution asynchronously
            tokio::spawn(async move {
                let default_actor = WasmlAddress::new([0; 32]);
                let wasml_actor = HyperTeeController::to_wasml_address(&default_actor);
                
                // Execute with proper handling of the mutex guard
                let result = {
                    // Acquire the mutex
                    let mut simulator = simulator_clone.write().await;
                    // Release the mutex before waiting for async operation
                    drop(simulator);
                    // Re-acquire the mutex for execution
                    let mut simulator = simulator_clone.write().await;
                    simulator.execute(&wasml_actor, &input_clone, &String::from_utf8_lossy(&function_call_clone.as_bytes()).to_string(), &[], DEFAULT_GAS).await
                };
                
                let mut operations = operations_clone.write().await;
                if let Some(op) = operations.get_mut(&operation_id_clone) {
                    match result {
                        Ok(res) => {
                            op.status = "completed".to_string();
                            op.result = Some(res);
                        },
                        Err(e) => {
                            op.status = "failed".to_string();
                            op.result = Some(format!("Error: {}", e).into_bytes());
                        }
                    }
                }
            });
            
            // Return a pending operation result
            return Ok(ExecutionResult {
                result: vec![], // No immediate result for async operations
                state_hash: vec![0; 32], // Mock state hash
                stats: ExecutionStats {
                    execution_time: 0,
                    memory_used: 0,
                    syscall_count: 0,
                },
                attestations: vec![TeeAttestation {
                    enclave_id: b"mock".to_vec(),
                    measurement: vec![0; 32],
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    signature: vec![0; 64],
                    region_proof: Some(vec![0; 32]),
                    data: vec![0; 32],
                    enclave_type: TeeType::SGX,
                }],
                timestamp: chrono::Utc::now().to_rfc3339(),
                operation_status: Some("pending".to_string()),
                operation_id: Some(operation_id_str),
                pending_operations: None,
            });
        }
        
        // Handle regular synchronous execution
        let default_actor = WasmlAddress::new([0; 32]); // Create a default actor address
        let wasml_actor = Self::to_wasml_address(&default_actor);
        let simulator_clone = self.simulator.clone();
        
        // Execute in a way that properly handles the mutex guard
        let result = {
            // Acquire the mutex
            let mut simulator = simulator_clone.write().await;
            // Drop the mutex guard before waiting for async operation
            drop(simulator);
            // Get the simulator again to ensure we don't hold the guard during await
            let mut simulator = simulator_clone.write().await;
            simulator
                .execute(&wasml_actor, &payload.input, payload.params.function_call.as_str(), &[], DEFAULT_GAS)
                .await
                .map_err(|e| TeeError::Contract(e.to_string()))?
        };
        
        // Return the execution result
        Ok(ExecutionResult {
            result,
            state_hash: vec![0; 32], // Mock state hash
            stats: ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
            },
            attestations: vec![TeeAttestation {
                enclave_id: b"mock".to_vec(),
                measurement: vec![0; 32],
                timestamp: chrono::Utc::now().timestamp() as u64,
                signature: vec![0; 64],
                region_proof: Some(vec![0; 32]),
                data: vec![0; 32],
                enclave_type: TeeType::SGX,
            }],
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: None,
            operation_id: None,
            pending_operations: None,
        })
    }

    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        self.deploy_contract(wasm_code, Some(region_id)).await
    }

    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
        // Return a mock region list
        Ok(vec![Region {
            id: "default".to_string(),
            worker_ids: vec!["simulator-1".to_string()],
            max_tasks: 100,
        }])
    }

    async fn get_attestations(&self, _region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        // Return a mock attestation
        Ok(vec![TeeAttestation {
            enclave_id: b"mock".to_vec(),
            measurement: vec![0; 32],
            timestamp: chrono::Utc::now().timestamp() as u64,
            signature: vec![0; 64],
            region_proof: Some(vec![0; 32]),
            data: vec![0; 32],
            enclave_type: TeeType::SGX,
        }])
    }

    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        let contracts = self.contracts.read().await;
        
        // Ensure contract exists
        if !contracts.contains_key(contract_address) {
            return Err(TeeError::Contract("Contract not found".to_string()));
        }
        
        // Generate mock state hash (in a real implementation this would be a state root)
        let mut hasher = Sha256::new();
        hasher.update(contract_address.as_bytes());
        hasher.update(chrono::Utc::now().timestamp().to_string().as_bytes());
        
        Ok(hasher.finalize().to_vec())
    }
}
