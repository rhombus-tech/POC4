use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, error, debug, warn};
use tee_interface::{TeeError, TeeExecutor, ExecutionPayload, ExecutionResult, TeeAttestation, Region, RegionInfo};
use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;
use chrono::Utc;
use crate::mesh::{MeshCoordinator, MeshExecutionResult, PeerInfo, SyncResult, TeeType as MeshTeeType};

/// TeeExecutorPair combines two TeeExecutor instances
/// for redundant execution and cross-checking results
pub struct TeeExecutorPair {
    /// Primary TEE executor
    primary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
    /// Secondary TEE executor
    secondary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
    /// Optional contract ID generator function for consistent IDs across TEEs
    contract_id_generator: Option<fn(&str) -> String>,
    /// Optional mesh coordinator for direct TEE-to-TEE communication
    mesh_coordinator: Option<Arc<MeshCoordinator>>,
}

impl TeeExecutorPair {
    /// Create a new TeeExecutorPair with primary and secondary executors
    pub fn new(
        primary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
        secondary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
        mesh_coordinator: Option<Arc<MeshCoordinator>>,
    ) -> Self {
        Self { 
            primary, 
            secondary,
            contract_id_generator: None,
            mesh_coordinator,
        }
    }
    
    /// Set a custom contract ID generator function
    pub fn with_contract_id_generator(mut self, generator: fn(&str) -> String) -> Self {
        self.contract_id_generator = Some(generator);
        self
    }
    
    /// Execute a task via the mesh network
    pub async fn execute_mesh(
        &self,
        target_tee: String,
        region: String,
        tee_type: MeshTeeType,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        info!("Executing task via mesh network: target={}, region={}", target_tee, region);
        
        match &self.mesh_coordinator {
            Some(coordinator) => {
                coordinator.execute(
                    target_tee, 
                    region, 
                    tee_type.to_string(),
                    input,
                    timeout,
                    is_async,
                    allow_fallback
                ).await
            },
            None => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Mesh coordinator not initialized"
                ))
            }
        }
    }
    
    /// Discover peers in the mesh network
    pub async fn discover_peers(
        &self,
        region: String,
        tee_type: Option<MeshTeeType>,
        max_results: usize,
    ) -> Result<Vec<PeerInfo>, std::io::Error> {
        info!("Discovering peers in region: {}", region);
        
        match &self.mesh_coordinator {
            Some(coordinator) => {
                coordinator.discover_peers(region, tee_type.map(|t| t.to_string()), max_results).await
            },
            None => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Mesh coordinator not initialized"
                ))
            }
        }
    }
    
    /// Synchronize state with another TEE
    pub async fn sync_state(
        &self,
        object_id: String,
        target_tee: String,
        use_deltas: bool,
    ) -> Result<SyncResult, std::io::Error> {
        info!("Synchronizing state with TEE: {}", target_tee);
        
        match &self.mesh_coordinator {
            Some(coordinator) => {
                coordinator.sync_state(object_id, target_tee, use_deltas).await
            },
            None => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Mesh coordinator not initialized"
                ))
            }
        }
    }
    
    /// Execute a task via the mesh network with caching
    pub async fn execute_with_mesh_cache(
        &self,
        target_tee: String,
        region: String,
        tee_type: MeshTeeType,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
        use_cache: bool,
        cache_ttl: Duration,
        stale_result_timeout: Duration,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        // If mesh coordinator is available, try to execute via mesh with caching
        if let Some(mesh) = &self.mesh_coordinator {
            info!("Executing via mesh with caching in region: {}, target: {}", region, target_tee);
            
            // Use execute_with_cache instead of execute_with_mesh_cache
            mesh.execute_with_cache(
                target_tee,
                region,
                tee_type.to_string(),
                input,
                timeout,
                is_async,
                allow_fallback,
                use_cache,
                cache_ttl,
                stale_result_timeout,
            ).await
        } else {
            error!("Mesh coordinator is not available for execution");
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Mesh coordinator is not available"
            ))
        }
    }
    
    /// Verify if both TEE platforms are available
    pub async fn verify_platforms(&self) -> (bool, bool) {
        debug!("Verifying platform availability");
        
        let sgx_available = match self.primary.read().await.get_attestations("default").await {
            Ok(_) => true,
            Err(_) => false,
        };
        
        let sev_available = match self.secondary.read().await.get_attestations("default").await {
            Ok(_) => true,
            Err(_) => false,
        };
        
        (sgx_available, sev_available)
    }
    
    /// Execute a task on both TEEs in paired mode
    pub async fn execute_paired(
        &self,
        wasm_module: PathBuf,
        input: Vec<u8>,
        contract_id: String,
        operation_id: String,
        _timeout: Duration,
        _use_cache: bool,
        function_call: Option<String>,
    ) -> Result<ExecutionResult, std::io::Error> {
        info!("Executing in paired mode with operation_id: {}", operation_id);
        
        // Read WASM module - keep this in case we need the code later
        let _wasm_code = match tokio::fs::read(&wasm_module).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read WASM module: {:?}", e);
                return Err(e);
            }
        };
        
        // Create execution payload with proper structure
        let mut payload = ExecutionPayload::default();
        
        // Set payload fields
        payload.operation_id = Some(operation_id.clone());
        payload.input = input;
        
        // Update the params
        payload.params.id_to = contract_id;
        payload.params.function_call = function_call.unwrap_or_else(|| "main".to_string());
        payload.params.detailed_proof = true;
        
        // Execute on both TEEs
        match self.execute(&payload).await {
            Ok(result) => Ok(result),
            Err(e) => {
                error!("Paired execution failed: {:?}", e);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Paired execution error: {}", e)
                ))
            }
        }
    }
}

#[async_trait]
impl TeeExecutor for TeeExecutorPair {
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        // If we have a custom contract ID generator, use it to generate a consistent ID
        if let Some(generator) = self.contract_id_generator {
            let contract_id = generator(region_id);
            info!("Using generated contract ID {} for region {}", contract_id, region_id);
            
            // Deploy to primary TEE with custom ID
            let primary_executor = self.primary.read().await;
            (*primary_executor).deploy_contract(wasm_code, region_id).await?;
            
            // Deploy to secondary TEE with custom ID
            let secondary_executor = self.secondary.read().await;
            (*secondary_executor).deploy_contract(wasm_code, region_id).await?;
            
            return Ok(contract_id);
        }
        
        // Standard deployment flow - deploy to both TEEs and ensure they match
        info!("Deploying contract to both TEEs in region {}", region_id);
        
        // Deploy to primary TEE
        let primary_executor = self.primary.read().await;
        let primary_id = (*primary_executor).deploy_contract(wasm_code, region_id).await?;
        
        // Deploy to secondary TEE
        let secondary_executor = self.secondary.read().await;
        let secondary_id = (*secondary_executor).deploy_contract(wasm_code, region_id).await?;
        
        // Verify both TEEs generated the same contract ID
        if primary_id != secondary_id {
            warn!("Contract IDs from primary and secondary TEEs don't match: {} vs {}", 
                 primary_id, secondary_id);
            return Err(TeeError::Contract("Contract IDs from primary and secondary TEEs don't match".to_string()));
        }
        
        info!("Contract deployed successfully on both TEEs with ID {}", primary_id);
        Ok(primary_id)
    }
    
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        info!("Executing contract on both TEEs for operation ID {}", 
              payload.operation_id.as_ref().unwrap_or(&"unknown".to_string()));
        
        // Track start time for overall execution
        let start_time = Utc::now();
        
        // Execute on primary TEE
        let primary_executor = self.primary.read().await;
        let primary_result = (*primary_executor).execute(payload).await?;
        
        // Execute on secondary TEE
        let secondary_executor = self.secondary.read().await;
        let secondary_result = (*secondary_executor).execute(payload).await?;
        
        // Calculate total execution time
        let execution_time = Utc::now().signed_duration_since(start_time).num_milliseconds() as u64;
        
        // Verify that both TEEs produced the same result
        if primary_result.result != secondary_result.result {
            error!("TEE execution results do not match!");
            return Err(TeeError::ExecutionError("TEE execution results do not match".to_string()));
        }
        
        // Combine attestations from both TEEs
        let mut combined_attestations = primary_result.attestations.clone();
        combined_attestations.extend(secondary_result.attestations);
        
        // Create result with combined attestations and stats
        let mut result = primary_result;
        result.attestations = combined_attestations;
        
        // Ensure execution stats are properly populated
        result.stats.execution_time = execution_time.max(result.stats.execution_time);
        
        info!("Contract executed successfully with matching results on both TEEs");
        Ok(result)
    }
    
    async fn get_regions(&self) -> Result<Vec<RegionInfo>, TeeError> {
        // Get regions from both TEEs
        let primary_executor = self.primary.read().await;
        let primary_regions = (*primary_executor).get_regions().await?;
        
        let secondary_executor = self.secondary.read().await;
        let secondary_regions = (*secondary_executor).get_regions().await?;
        
        // Combine regions from both TEEs
        let mut all_regions = primary_regions;
        all_regions.extend(secondary_regions);
        
        Ok(all_regions)
    }
    
    async fn get_attestations(&self, region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        // Get attestations from both TEEs
        let primary_executor = self.primary.read().await;
        let primary_attestations = (*primary_executor).get_attestations(region_id).await?;
        
        let secondary_executor = self.secondary.read().await;
        let secondary_attestations = (*secondary_executor).get_attestations(region_id).await?;
        
        // Combine attestations from both TEEs
        let mut all_attestations = primary_attestations;
        all_attestations.extend(secondary_attestations);
        
        Ok(all_attestations)
    }
    
    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        // Get state hash from primary TEE
        let primary_executor = self.primary.read().await;
        let primary_hash = (*primary_executor).get_state_hash(contract_address).await?;
        
        // Get state hash from secondary TEE
        let secondary_executor = self.secondary.read().await;
        let secondary_hash = (*secondary_executor).get_state_hash(contract_address).await?;
        
        // Verify both hashes match
        if primary_hash != secondary_hash {
            error!("State hash mismatch between primary and secondary TEEs!");
            return Err(TeeError::Contract("State hash mismatch between primary and secondary TEEs".to_string()));
        }
        
        // Return the hash (they're identical)
        Ok(primary_hash)
    }
}
