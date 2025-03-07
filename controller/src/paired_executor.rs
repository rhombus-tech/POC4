use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, warn, error};
use tee_interface::{TeeError, TeeExecutor, ExecutionPayload, ExecutionResult, TeeAttestation, Region};
use async_trait::async_trait;

/// TeeExecutorPair combines two TeeExecutor instances
/// for redundant execution and cross-checking results
pub struct TeeExecutorPair {
    /// Primary TEE executor
    primary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
    /// Secondary TEE executor
    secondary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
}

impl TeeExecutorPair {
    /// Create a new TeeExecutorPair with primary and secondary executors
    pub fn new(
        primary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
        secondary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>
    ) -> Self {
        Self { primary, secondary }
    }
}

#[async_trait]
impl TeeExecutor for TeeExecutorPair {
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        // Deploy to both TEEs and ensure they match
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
        info!("Executing contract on both TEEs for operation {}", 
              payload.operation_id.as_deref().unwrap_or("unknown"));
        
        // Execute on primary TEE
        let primary_executor = self.primary.read().await;
        let primary_result = (*primary_executor).execute(payload).await?;
        
        // Execute on secondary TEE
        let secondary_executor = self.secondary.read().await;
        let secondary_result = (*secondary_executor).execute(payload).await?;
        
        // Verify that both TEEs produced the same result
        if primary_result.result != secondary_result.result {
            error!("TEE execution results do not match!");
            return Err(TeeError::ExecutionError("TEE execution results do not match".to_string()));
        }
        
        // Combine attestations from both TEEs
        let mut combined_attestations = primary_result.attestations.clone();
        combined_attestations.extend(secondary_result.attestations);
        
        // Create result with combined attestations
        let mut result = primary_result;
        result.attestations = combined_attestations;
        
        info!("Contract executed successfully with matching results on both TEEs");
        Ok(result)
    }
    
    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
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
