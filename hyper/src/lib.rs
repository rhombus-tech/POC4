use tee_interface::prelude::*;
use tee_interface::types::ExecutionResult;
use tee_interface::ContractState;
use tee_interface::TeeController;
use thiserror::Error;
use std::sync::Arc;
use tokio::sync::RwLock;
use borsh_derive::{BorshSerialize, BorshDeserialize};
use std::collections::HashMap;

pub mod runtime;
pub mod test_utils;

pub use runtime::{HyperSDKRuntime, RuntimeConfig, StateManager};

pub type Result<T> = std::result::Result<T, TeeError>;

#[derive(Debug, Error)]
pub enum HyperError {
    #[error("Contract execution failed: {0}")]
    ExecutionError(String),
    #[error("Contract state error: {0}")]
    StateError(String),
}

impl From<HyperError> for TeeError {
    fn from(err: HyperError) -> Self {
        match err {
            HyperError::ExecutionError(msg) => TeeError::ExecutionError(msg),
            HyperError::StateError(msg) => TeeError::ExecutionError(msg),
        }
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionInput {
    pub wasm_bytes: Vec<u8>,
    pub function: String,
    pub args: Vec<u8>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ContractContext {
    pub contract_id: [u8; 32],
    pub timestamp: u64,
    pub nonce: u64,
}

pub struct HyperExecutor {
    controller: Arc<dyn TeeController>,
    runtime: HyperSDKRuntime,
    rate_limiter: Arc<RwLock<HashMap<String, u64>>>,
}

impl std::fmt::Debug for HyperExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HyperExecutor")
            .field("rate_limiter", &self.rate_limiter)
            .finish()
    }
}

impl HyperExecutor {
    pub async fn new(controller: Arc<dyn TeeController>, state_manager: Arc<dyn StateManager>) -> Result<Self> {
        let config = RuntimeConfig::default();
        let runtime = HyperSDKRuntime::new(state_manager, config)?;
        
        Ok(Self {
            controller,
            runtime,
            rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn execute_contract(
        &self,
        region_id: String,
        contract_id: [u8; 32],
        input: ExecutionInput,
    ) -> Result<ExecutionResult> {
        // Rate limit check
        let mut limiter = self.rate_limiter.write().await;
        let last_execution = limiter.get(&region_id).copied().unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now - last_execution < 1 {
            return Err(TeeError::ExecutionError("Rate limit exceeded".to_string()));
        }
        limiter.insert(region_id.clone(), now);
        drop(limiter);

        // 1. Deploy contract if needed
        let contract_id = if contract_id == [0u8; 32] {
            self.runtime.deploy_contract(&input.wasm_bytes).await
                .map_err(|e| TeeError::ExecutionError(format!("Failed to deploy contract: {}", e)))?
        } else {
            contract_id
        };

        // 2. Execute through HyperSDK runtime
        let result = self.runtime
            .call_contract(contract_id, &input.function, &input.args).await
            .map_err(|e| TeeError::ExecutionError(format!("Contract execution failed: {}", e)))?;

        // 3. Get attestation from TEE
        let tee_result = self.controller
            .execute(
                region_id.clone(),
                result.clone(),
                true,
            )
            .await?;

        Ok(ExecutionResult {
            tx_id: vec![],
            state_hash: [0u8; 32],
            output: result,
            attestations: [tee_result.attestations[0].clone(), tee_result.attestations[1].clone()],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            region_id,
        })
    }

    pub async fn get_contract_state(
        &self,
        contract_id: [u8; 32],
    ) -> Result<ContractState> {
        self.runtime.get_contract_state(contract_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tee_interface::types::{TeeType, ExecutionResult, TeeAttestation, PlatformMeasurement};
    use tee_interface::ContractState;
    use tee_interface::prelude::Result;

    #[derive(Debug, Default)]
    pub struct MockController;

    #[async_trait]
    impl TeeController for MockController {
        async fn execute(
            &self,
            region_id: String,
            input: Vec<u8>,
            _verify: bool,
        ) -> Result<ExecutionResult> {
            let attestation1 = TeeAttestation {
                tee_type: TeeType::Sgx,
                measurement: PlatformMeasurement::Sgx {
                    mrenclave: [0u8; 32],
                    mrsigner: [0u8; 32],
                    attributes: [0u8; 16],
                    miscselect: 0,
                },
                signature: vec![0u8; 64],
            };

            let attestation2 = attestation1.clone();

            Ok(ExecutionResult {
                tx_id: vec![],
                state_hash: [0u8; 32],
                output: input,
                attestations: [attestation1, attestation2],
                timestamp: 0,
                region_id,
            })
        }

        async fn health_check(&self) -> Result<bool> {
            Ok(true)
        }

        async fn get_state(&self, _region_id: String, _contract_id: [u8; 32]) -> Result<ContractState> {
            Ok(ContractState {
                state: vec![],
                timestamp: 0,
            })
        }
    }
}
