use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeConfig, TeeError, TeeAttestation, Region, ExecutionResult};
use futures::try_join;

pub struct TeeExecutorPair {
    primary: Arc<RwLock<Box<dyn TeeExecutor + Send + Sync>>>,
    secondary: Arc<RwLock<Box<dyn TeeExecutor + Send + Sync>>>,
}

impl TeeExecutorPair {
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
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let primary = self.primary.read().await;
        let secondary = self.secondary.read().await;

        // Execute in both TEEs concurrently
        let (primary_result, secondary_result) = try_join!(
            primary.execute(payload),
            secondary.execute(payload)
        )?;

        // Verify results match
        if primary_result.result != secondary_result.result {
            return Err(TeeError::ExecutionError("Results from primary and secondary do not match".into()));
        }

        Ok(primary_result)
    }

    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
        // Only return primary regions since both should be identical
        self.primary.read().await.get_regions().await
    }

    async fn get_attestations(&self, region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        let primary = self.primary.read().await;
        let secondary = self.secondary.read().await;

        // Get attestations from both TEEs concurrently
        let (primary_att, secondary_att) = try_join!(
            primary.get_attestations(region_id),
            secondary.get_attestations(region_id)
        )?;

        // Return combined attestations
        let mut attestations = primary_att;
        attestations.extend(secondary_att);
        Ok(attestations)
    }

    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        let primary = self.primary.read().await;
        let secondary = self.secondary.read().await;

        // Deploy to both TEEs concurrently
        let (primary_id, _) = try_join!(
            primary.deploy_contract(wasm_code, region_id),
            secondary.deploy_contract(wasm_code, region_id)
        )?;

        // Return primary contract ID since both should be identical
        Ok(primary_id)
    }

    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        // Only return primary state hash since both should be identical
        self.primary.read().await.get_state_hash(contract_address).await
    }
}
