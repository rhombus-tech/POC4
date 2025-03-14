use log::{debug, info};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::{TeeError, TeeExecutor, ExecutionPayload, ExecutionResult, TeeAttestation, RegionInfo, TeeType};
use crate::enarx::keep_manager::{KeepManager, KeepManagerConfig};
use crate::enarx::param_handler::ParamHandler;
use crate::enarx::error::EnarxError;
use chrono;
use hex;
use sha2::{Sha256, Digest};
use crate::simulator::SimulatorController;

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
    /// TEE type
    tee_type: TeeType,
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
                tee_type,
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
                warm_keeps: 3,  // Increase default warm keeps to 3 for better performance
                keep_max_lifetime: 3600, // 1 hour
                keep_max_idle_time: 300, // 5 minutes
                init_timeout_ms: 5000,   // 5 seconds
            };
            let keep_manager = Some(KeepManager::new(config));
            
            Self {
                config_dir: config_dir.to_string(),
                enarx_path,
                keep_manager,
                simulation,
                simulator: None,
                contracts: Arc::new(RwLock::new(std::collections::HashMap::new())),
                tee_type,
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
            fs::create_dir_all(&self.config_dir).map_err(|e| {
                EnarxError::KeepManagerError(format!("Failed to create config directory: {}", e))
            })?;
        }
        
        // Initialize the keep manager
        if let Some(km) = &self.keep_manager {
            km.initialize().await.map_err(|e| {
                EnarxError::KeepManagerError(format!("Failed to initialize keep manager: {}", e))
            })?;
        }
        
        Ok(())
    }
    
    /// Helper method to save contract to disk
    async fn save_contract(&self, contract_id: &str, wasm_bytes: &[u8]) -> Result<PathBuf, EnarxError> {
        // Create a file path for the contract
        let contract_filename = format!("{}.wasm", contract_id);
        let contract_path = PathBuf::from(&self.config_dir).join(&contract_filename);
        
        // Write the contract to disk
        fs::write(&contract_path, wasm_bytes)
            .map_err(|e| EnarxError::FileError(format!("Failed to write contract to disk: {}", e)))?;
        
        // Add to contracts map
        let mut contracts = self.contracts.write().await;
        contracts.insert(contract_id.to_string(), contract_path.clone());
        
        Ok(contract_path)
    }
    
    /// Get state hash for a contract
    async fn compute_state_hash(&self, contract_id: &str) -> Result<Vec<u8>, EnarxError> {
        // For now, just use a simple hash of the contract ID
        // In a real implementation, this would compute a hash of the contract state
        let mut hasher = Sha256::new();
        hasher.update(contract_id.as_bytes());
        let hash = hasher.finalize();
        
        Ok(hash.to_vec())
    }
    
    /// Helper method to get attestation
    async fn get_attestation(&self, region_id: &str) -> Result<TeeAttestation, EnarxError> {
        // In a real implementation, this would get actual attestation from the TEE
        // For now, return a simulated attestation
        let attestation = TeeAttestation {
            enclave_id: b"sim-enclave-id".to_vec(),
            measurement: hex::decode("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20").unwrap(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: b"simulator-attestation-data".to_vec(),
            signature: vec![0; 64],
            region_proof: Some(vec![]),
            enclave_type: self.tee_type.clone(),
        };
        
        Ok(attestation)
    }
}

#[async_trait::async_trait]
impl TeeExecutor for EnarxController {
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        debug!("Executing contract with {} bytes of input data", payload.input.len());
        
        if self.simulation {
            // Use the simulator for simulation mode
            if let Some(simulator) = &self.simulator {
                return simulator.execute(payload).await;
            } else {
                return Err(TeeError::ExecutionError("Simulator not initialized".to_string()));
            }
        }
        
        // Get the contract file path
        let contract_path = {
            let contracts = self.contracts.read().await;
            match contracts.get(&payload.params.id_to) {
                Some(path) => path.clone(),
                None => {
                    return Err(TeeError::ExecutionError(
                        format!("Contract not found: {}", payload.params.id_to)
                    ));
                }
            }
        };
        
        // Get the keep manager
        let keep_manager = match &self.keep_manager {
            Some(km) => km,
            None => {
                return Err(TeeError::ExecutionError("Keep Manager not initialized".to_string()));
            }
        };
        
        // Prepare the parameters using our new encoding method
        let params = ParamHandler::encode(payload).map_err(|e| {
            TeeError::ExecutionError(format!("Failed to encode parameters: {}", e))
        })?;
        
        debug!("Executing contract with {} bytes of encoded parameters", params.len());
        
        // Execute the contract
        let start_time = std::time::Instant::now();
        let result = keep_manager.execute(&contract_path, &params).await?;
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        // Create basic execution stats
        let stats = tee_interface::ExecutionStats {
            execution_time,
            memory_used: 0, // Not tracked in our current implementation
            syscall_count: 0, // Not tracked in our current implementation
            network_latency: 0, // Not applicable for Enarx execution
            custom_metrics: None, // No custom metrics yet
        };
        
        // Get attestation
        let attestation = self.get_attestation("default-region").await
            .map_err(|e| TeeError::Attestation(format!("Failed to get attestation: {}", e)))?;
            
        // Get state hash
        let state_hash = self.compute_state_hash(&payload.params.id_to)
            .await
            .map_err(|e| TeeError::ExecutionError(format!("Failed to compute state hash: {}", e)))?;
            
        Ok(ExecutionResult {
            result,
            attestations: vec![attestation],
            state_hash,
            stats,
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: None,
            operation_id: None,
            pending_operations: None,
        })
    }
    
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        info!("Deploying contract to region {}", region_id);
        
        // Generate a contract ID
        let contract_id = format!("contract-{}", hex::encode(&wasm_code[0..4]));
        
        if self.simulation {
            info!("Simulating contract deployment: {}", contract_id);
            return Ok(contract_id);
        }
        
        // Save the contract to disk
        self.save_contract(&contract_id, wasm_code)
            .await
            .map_err(|e| TeeError::ExecutionError(format!("Failed to save contract: {}", e)))?;
            
        info!("Contract {} deployed successfully", contract_id);
        
        Ok(contract_id)
    }
    
    async fn get_regions(&self) -> Result<Vec<RegionInfo>, TeeError> {
        // In a real implementation, this would get actual regions from a registry
        // For now, return a simulated region
        let region = RegionInfo {
            id: "default-region".to_string(),
            worker_ids: vec!["worker-1".to_string(), "worker-2".to_string()],
            max_tasks: 100,
        };
        
        Ok(vec![region])
    }
    
    async fn get_attestations(&self, region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        let attestation = self.get_attestation(region_id)
            .await
            .map_err(|e| TeeError::Attestation(format!("Failed to get attestation: {}", e)))?;
            
        Ok(vec![attestation])
    }
    
    async fn get_state_hash(&self, contract_id: &str) -> Result<Vec<u8>, TeeError> {
        self.compute_state_hash(contract_id)
            .await
            .map_err(|e| TeeError::ExecutionError(format!("Failed to compute state hash: {}", e)))
    }
}