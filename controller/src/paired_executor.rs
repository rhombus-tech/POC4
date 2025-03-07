use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeConfig, TeeError, TeeAttestation, Region, ExecutionResult, ExecutionStats};
use futures::try_join;

/// A pair of TEE executors that run in parallel for redundancy and verification.
/// 
/// This executor executes the same payload on both primary and secondary executors
/// and verifies that the results match. This provides additional security and fault tolerance
/// by ensuring that two independent TEEs produce the same execution result.
pub struct TeeExecutorPair {
    /// The primary TEE executor
    primary: Arc<RwLock<Box<dyn TeeExecutor + Send + Sync>>>,
    /// The secondary TEE executor (for verification)
    secondary: Arc<RwLock<Box<dyn TeeExecutor + Send + Sync>>>,
}

impl TeeExecutorPair {
    /// Creates a new pair of TEE executors
    ///
    /// # Arguments
    /// 
    /// * `primary` - The primary TEE executor
    /// * `secondary` - The secondary TEE executor used for verification
    ///
    /// # Returns
    ///
    /// A new TeeExecutorPair instance
    pub fn new(
        primary: Box<dyn TeeExecutor + Send + Sync>,
        secondary: Box<dyn TeeExecutor + Send + Sync>,
    ) -> Self {
        Self {
            primary: Arc::new(RwLock::new(primary)),
            secondary: Arc::new(RwLock::new(secondary)),
        }
    }
}

#[async_trait::async_trait]
impl TeeExecutor for TeeExecutorPair {
    /// Executes a payload on both TEE executors and verifies the results match
    ///
    /// # Arguments
    ///
    /// * `payload` - The execution payload to run
    ///
    /// # Returns
    ///
    /// The execution result from the primary executor if both results match
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Clone Arc to avoid holding read lock across await points
        let primary_clone = self.primary.clone();
        let secondary_clone = self.secondary.clone();
        
        // Execute in both TEEs concurrently
        let primary_future = async move {
            let guard = primary_clone.read().await;
            guard.execute(payload).await
        };
        
        let secondary_future = async move {
            let guard = secondary_clone.read().await;
            guard.execute(payload).await
        };
        
        let (primary_result, secondary_result) = try_join!(
            primary_future,
            secondary_future
        )?;

        // Verify results match
        if primary_result.result != secondary_result.result {
            return Err(TeeError::ExecutionError("Results from primary and secondary do not match".into()));
        }

        Ok(primary_result)
    }

    /// Get the available regions from the primary executor
    ///
    /// # Returns
    ///
    /// A list of available regions
    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
        // Clone Arc to avoid holding read lock across await points
        let primary_clone = self.primary.clone();
        
        // Use async block to avoid holding the lock across await points
        let regions_future = async move {
            let guard = primary_clone.read().await;
            guard.get_regions().await
        };
        
        // Only return primary regions since both should be identical
        regions_future.await
    }

    /// Get attestations from both primary and secondary executors
    ///
    /// # Arguments
    ///
    /// * `region_id` - The region ID to get attestations for
    ///
    /// # Returns
    ///
    /// Combined attestations from both executors
    async fn get_attestations(&self, region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        // Clone Arc to avoid holding read lock across await points
        let primary_clone = self.primary.clone();
        let secondary_clone = self.secondary.clone();
        
        // Get attestations from both TEEs concurrently
        let primary_future = async move {
            let guard = primary_clone.read().await;
            guard.get_attestations(region_id).await
        };
        
        let secondary_future = async move {
            let guard = secondary_clone.read().await;
            guard.get_attestations(region_id).await
        };
        
        let (primary_att, secondary_att) = try_join!(
            primary_future,
            secondary_future
        )?;

        // Return combined attestations
        let mut attestations = primary_att;
        attestations.extend(secondary_att);
        Ok(attestations)
    }

    /// Deploy a contract to both primary and secondary executors
    ///
    /// # Arguments
    ///
    /// * `wasm_code` - The WebAssembly contract code to deploy
    /// * `region_id` - The region ID to deploy to
    ///
    /// # Returns
    ///
    /// The contract ID from the primary executor
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        // Clone Arc to avoid holding read lock across await points
        let primary_clone = self.primary.clone();
        let secondary_clone = self.secondary.clone();
        
        // Deploy to both TEEs concurrently
        let primary_future = async move {
            let guard = primary_clone.read().await;
            guard.deploy_contract(wasm_code, region_id).await
        };
        
        let secondary_future = async move {
            let guard = secondary_clone.read().await;
            guard.deploy_contract(wasm_code, region_id).await
        };
        
        let (primary_id, _) = try_join!(
            primary_future,
            secondary_future
        )?;

        // Return primary contract ID since both should be identical
        Ok(primary_id)
    }

    /// Get the state hash for a contract from the primary executor
    ///
    /// # Arguments
    ///
    /// * `contract_address` - The contract address to get state hash for
    ///
    /// # Returns
    ///
    /// The state hash from the primary executor
    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        // Clone Arc to avoid holding read lock across await points
        let primary_clone = self.primary.clone();
        
        // Use async block to avoid holding the lock across await points
        let hash_future = async move {
            let guard = primary_clone.read().await;
            guard.get_state_hash(contract_address).await
        };
        
        // Only return primary state hash since both should be identical
        hash_future.await
    }
}
