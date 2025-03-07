use log::{debug, info, warn, error};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::{TeeError, TeeExecutor, ExecutionPayload, ExecutionResult, TeeAttestation, Region};
use crate::enarx::keep_manager::{KeepManager, KeepManagerConfig};
use crate::enarx::param_handler::ParamHandler;
use crate::enarx::error::EnarxError;
use chrono;
use hex;
use sha2::{Sha256, Digest};

/// Controller for Enarx TEE
pub struct EnarxController {
    /// Keep Manager for handling Enarx keeps
    keep_manager: Option<Arc<KeepManager>>,
    /// Config directory for storing contracts
    config_dir: PathBuf,
    /// Current TEE type (SGX or SEV)
    tee_type: String,
    /// Simulation mode
    simulation: bool,
    /// Map of contract IDs to filenames
    contracts: Arc<RwLock<std::collections::HashMap<String, PathBuf>>>,
}

impl EnarxController {
    /// Create a new Enarx controller
    pub fn new(tee_type: String, config_dir: PathBuf, simulate: bool) -> Self {
        info!("Creating EnarxController for TEE type: {}, config_dir: {:?}, simulate: {}", 
              tee_type, config_dir, simulate);
        
        Self {
            keep_manager: None,
            config_dir,
            tee_type,
            simulation: simulate,
            contracts: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// Initialize the Enarx controller
    pub async fn initialize(&mut self) -> Result<(), EnarxError> {
        if self.simulation {
            info!("Initializing EnarxController in simulation mode");
            return Ok(());
        }
        
        info!("Initializing EnarxController with real Enarx Keep Manager");
        
        // Ensure the config directory exists
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)
                .map_err(|e| EnarxError::Other(format!("Failed to create config directory: {}", e)))?;
        }
        
        // Initialize keep manager
        let config = KeepManagerConfig::default();
        let keep_manager = Arc::new(KeepManager::new(config));
        
        // Initialize the keep manager
        keep_manager.initialize().await?;
        
        self.keep_manager = Some(keep_manager);
        
        Ok(())
    }
    
    /// Helper method to save contract to disk
    async fn save_contract(&self, contract_id: &str, wasm_bytes: &[u8]) -> Result<PathBuf, EnarxError> {
        // Create a directory for the contract if it doesn't exist
        let contract_dir = self.config_dir.join(contract_id);
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
            enclave_type: tee_interface::TeeType::SGX,
        };
        
        Ok(vec![attestation])
    }
}

#[async_trait::async_trait]
impl TeeExecutor for EnarxController {
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        debug!("Deploying contract with {} bytes to region {}", wasm_code.len(), region_id);
        
        // Generate a contract ID (in real implementation, this would be a hash of the WASM)
        let mut hasher = Sha256::new();
        hasher.update(wasm_code);
        let contract_id = hex::encode(hasher.finalize());
        
        let wasm_path = self.save_contract(&contract_id, wasm_code).await?;
        
        // Store contract ID mapping
        let mut contracts = self.contracts.write().await;
        contracts.insert(contract_id.clone(), wasm_path);
        
        info!("Deployed contract {}", contract_id);
        
        // Return contract ID
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
            // In simulation mode, just return the params as the result
            debug!("Simulation mode: returning params as result");
            processed_params.to_vec()
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
            format!("{}-region", self.tee_type.to_lowercase())
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
            id: format!("{}-region", self.tee_type.to_lowercase()),
            worker_ids: vec![format!("{}-worker-1", self.tee_type.to_lowercase())],
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