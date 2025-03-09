use log::{debug, info};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::{TeeError, TeeExecutor, ExecutionPayload, ExecutionResult, TeeAttestation, Region, TeeType};
use crate::enarx::keep_manager::{KeepManager, KeepManagerConfig};
use crate::enarx::param_handler::ParamHandler;
use crate::enarx::error::EnarxError;
use chrono;
use hex;
use sha2::{Sha256, Digest};
use crate::simulator::SimulatorController;
use futures::TryFutureExt;

/// Controller for Enarx TEE
pub struct EnarxController {
    /// Config directory for storing contracts
    config_dir: String,
    /// Enarx path
    enarx_path: Option<String>,
    /// Keep Manager for handling Enarx keeps
    keep_manager: Option<KeepManager>,
    /// Simulation mode
    simulation: bool,
    /// Simulator for simulation mode
    simulator: Option<SimulatorController>,
    /// Map of contract IDs to filenames
    contracts: Arc<RwLock<std::collections::HashMap<String, PathBuf>>>,
}

impl EnarxController {
    /// Create a new Enarx controller
    pub async fn new(tee_type: TeeType, config_dir: &str, simulation: bool) -> Result<Self, TeeError> {
        info!("Creating EnarxController for TEE type: {:?}, config_dir: {:?}, simulate: {}", tee_type, config_dir, simulation);
        
        let controller = if simulation {
            info!("Initializing EnarxController in simulation mode");
            let simulator = Some(SimulatorController::new().await);
            Self {
                config_dir: config_dir.to_string(),
                enarx_path: None,
                keep_manager: None,
                simulation,
                simulator,
                contracts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            }
        } else {
            // Original code for non-simulation mode
            let enarx_path = std::env::var("ENARX_PATH").ok();
            
            // If enarx_path is not set, try to find enarx in PATH
            let enarx_path = match enarx_path {
                Some(path) => Some(path),
                None => {
                    if which::which("enarx").is_ok() {
                        Some("enarx".to_string())
                    } else {
                        return Err(TeeError::ExecutionError("Enarx path not provided and not found in PATH".to_string()));
                    }
                }
            };
            
            let config = KeepManagerConfig { 
                enarx_path: enarx_path.clone().unwrap(), 
                max_keeps: 10,
                warm_keeps: 2,
            };
            let keep_manager = Some(KeepManager::new(config));
            
            Self {
                config_dir: config_dir.to_string(),
                enarx_path,
                keep_manager,
                simulation,
                simulator: None,
                contracts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            }
        };
        
        Ok(controller)
    }
    
    /// Initialize the Enarx controller
    pub async fn initialize(&mut self) -> Result<(), EnarxError> {
        if self.simulation {
            info!("Initializing EnarxController in simulation mode");
            return Ok(());
        }
        
        info!("Initializing EnarxController with real Enarx Keep Manager");
        
        // Ensure the config directory exists
        if !PathBuf::from(&self.config_dir).exists() {
            fs::create_dir_all(&self.config_dir)
                .map_err(|e| EnarxError::Other(format!("Failed to create config directory: {}", e)))?;
        }
        
        // Initialize keep manager
        if let Some(keep_manager) = &mut self.keep_manager {
            keep_manager.initialize().await?;
        }
        
        Ok(())
    }
    
    /// Helper method to save contract to disk
    async fn save_contract(&self, contract_id: &str, wasm_bytes: &[u8]) -> Result<PathBuf, EnarxError> {
        // Create a directory for the contract if it doesn't exist
        let contract_dir = PathBuf::from(&self.config_dir).join(contract_id);
        if !contract_dir.exists() {
            fs::create_dir_all(&contract_dir)
                .map_err(|e| EnarxError::DeploymentError(format!("Failed to create contract directory: {}", e)))?;
        }
        
        // Write the WASM file
        let wasm_path = contract_dir.join("contract.wasm");
        fs::write(&wasm_path, wasm_bytes)
            .map_err(|e| EnarxError::DeploymentError(format!("Failed to write contract file: {}", e)))?;
        
        Ok(wasm_path)
    }
    
    /// Get state hash for a contract
    fn compute_state_hash(&self, contract_id: &str) -> Result<Vec<u8>, EnarxError> {
        // In a real implementation, we would compute a hash of the contract state
        // For simplicity, just return a dummy hash based on the contract ID
        let mut hash = Vec::new();
        hash.extend_from_slice(contract_id.as_bytes());
        while hash.len() < 32 {
            hash.push(0);
        }
        Ok(hash[0..32].to_vec())
    }
    
    /// Helper method to get attestation
    fn get_attestation(&self, region_id: &str) -> Result<Vec<TeeAttestation>, EnarxError> {
        // In a real implementation, we would get a real attestation report
        // For now, return a dummy attestation report
        let attestation = TeeAttestation {
            enclave_id: vec![0; 16],
            measurement: vec![0; 32],
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: vec![],
            signature: vec![0; 64],
            region_proof: None,
            enclave_type: TeeType::SGX,
        };
        
        Ok(vec![attestation])
    }
}

#[async_trait::async_trait]
impl TeeExecutor for EnarxController {
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        // Generate a unique contract ID
        let mut hasher = Sha256::new();
        hasher.update(wasm_code);
        let contract_id = hex::encode(hasher.finalize());
        
        info!("Deployed contract {}", contract_id);
        
        // Store the contract for simulation mode if needed
        if self.simulation {
            if let Some(simulator) = &self.simulator {
                // Deploy the contract to the simulator
                simulator.deploy_contract(wasm_code, region_id).await?;
                
                // Write the WASM code to a file for later use
                let wasm_path = format!("{}/contract_{}.wasm", self.config_dir, contract_id);
                tokio::fs::create_dir_all(&self.config_dir).await
                    .map_err(|e| TeeError::Contract(format!("Failed to create config directory: {}", e)))?;
                
                tokio::fs::write(&wasm_path, wasm_code).await
                    .map_err(|e| TeeError::Contract(format!("Failed to write WASM file: {}", e)))?;
                
                // Store the contract reference in our map
                let mut contracts = self.contracts.write().await;
                contracts.insert(contract_id.clone(), PathBuf::from(&wasm_path));
            }
        } else {
            // Save the contract to a file
            let wasm_path = format!("{}/contract_{}.wasm", self.config_dir, contract_id);
            
            tokio::fs::create_dir_all(&self.config_dir).await
                .map_err(|e| TeeError::Contract(format!("Failed to create config directory: {}", e)))?;
            
            tokio::fs::write(&wasm_path, wasm_code).await
                .map_err(|e| TeeError::Contract(format!("Failed to write WASM file: {}", e)))?;
                
            // Store the contract in the map
            let mut contracts = self.contracts.write().await;
            contracts.insert(contract_id.clone(), PathBuf::from(&wasm_path));
        }
        
        Ok(contract_id)
    }
    
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let contract_id = &payload.params.id_to;
        let method = &payload.params.function_call;
        let input = &payload.input;
        
        info!("Executing contract {} method {} with {} bytes of data", 
              contract_id, method, input.len());
        
        // Get the contract file path
        let contracts = self.contracts.read().await;
        let wasm_path = contracts.get(contract_id).ok_or_else(|| {
            TeeError::Contract(format!("Contract {} not found", contract_id))
        })?;
        
        let param_handler = ParamHandler::new();
        let processed_params = param_handler.process_params(input)?;
        
        let result = if self.simulation {
            if let Some(simulator) = &self.simulator {
                // In simulation mode, execute using the simulator
                let execution_result = simulator.execute(payload).await
                    .map_err(|e| TeeError::ExecutionError(format!("Simulation execution failed: {}", e)))?;
                execution_result.result
            } else {
                return Err(TeeError::ExecutionError("Simulator not initialized".to_string()));
            }
        } else {
            // Real execution using the keep manager
            if let Some(keep_manager) = &self.keep_manager {
                keep_manager.execute(wasm_path, &processed_params).await?
            } else {
                return Err(TeeError::ExecutionError("Keep Manager not initialized".to_string()));
            }
        };
        
        // Extract region_id from the payload or use a default
        let region_id = if let Some(region) = payload.params.id_to.split('/').next() {
            region.to_string()
        } else {
            format!("{}-region", "enarx".to_lowercase())
        };
        
        // Return the execution result
        Ok(ExecutionResult {
            result,
            state_hash: self.compute_state_hash(contract_id)?,
            stats: tee_interface::ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
            },
            attestations: self.get_attestation(&region_id)?,
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: Some("completed".to_string()),
            operation_id: payload.operation_id.clone(),
            pending_operations: None,  // No pending operations
        })
    }
    
    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
        // Return a dummy region for now
        Ok(vec![Region {
            id: format!("{}-region", "enarx".to_lowercase()),
            worker_ids: vec![format!("{}-worker-1", "enarx".to_lowercase())],
            max_tasks: 10,
        }])
    }
    
    async fn get_attestations(&self, region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(self.get_attestation(region_id)?)
    }
    
    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        Ok(self.compute_state_hash(contract_address)?)
    }
}