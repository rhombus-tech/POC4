use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::Request;
use tee_interface::{prelude::*, TeeController};
use crate::server::TeeExecutionService;
use crate::server::teeservice::tee_execution_server::TeeExecution;
use crate::server::teeservice::{ExecutionRequest, ExecutionResult as ProtoExecutionResult, TeeAttestation};
use sha2::{Sha256, Digest};

/// Controller that integrates TEE execution with HyperSDK
pub struct HyperTeeController {
    service: Arc<RwLock<TeeExecutionService>>,
    region_id: String,
}

impl HyperTeeController {
    pub fn new(service: Arc<RwLock<TeeExecutionService>>) -> Self {
        Self { 
            service,
            region_id: "default".to_string(),
        }
    }

    /// Set the region ID for TEE execution
    pub fn set_region(&mut self, region_id: String) {
        self.region_id = region_id;
    }

    /// Convert proto attestation to interface attestation
    fn convert_attestation(&self, proto_attestation: &TeeAttestation) -> tee_interface::TeeAttestation {
        let mut hasher = Sha256::new();
        hasher.update(&proto_attestation.data);
        let enclave_id = hasher.finalize().into();

        tee_interface::TeeAttestation {
            enclave_id,
            measurement: proto_attestation.data.clone(),
            data: proto_attestation.data.clone(),
            signature: proto_attestation.signature.clone(),
            region_proof: None,
        }
    }
}

#[async_trait::async_trait]
impl TeeController for HyperTeeController {
    async fn init(&mut self) -> Result<(), TeeError> {
        // Initialize connection to TEE service
        let service = self.service.read().await;
        
        // Check if region exists
        if !service.regions.contains_key(&self.region_id) {
            return Err(TeeError::VerificationError(format!(
                "Region {} not found",
                self.region_id
            )));
        }

        Ok(())
    }

    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let request = ExecutionRequest {
            id_to: payload.execution_id.to_string(),
            function_call: "execute".to_string(),
            parameters: payload.input.clone(),
            region_id: self.region_id.clone(),
            detailed_proof: payload.params.detailed_proof,
            expected_hash: payload.params.expected_hash.unwrap_or_default().to_vec(),
        };

        let service = self.service.read().await;
        let result = service
            .execute(Request::new(request))
            .await
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?
            .into_inner();

        let attestation = if let Some(att) = result.attestations.first() {
            self.convert_attestation(att)
        } else {
            return Err(TeeError::VerificationError("No attestation provided".to_string()));
        };

        Ok(ExecutionResult {
            result: result.result,
            attestation,
            state_hash: result.state_hash,
            stats: ExecutionStats {
                execution_time: result.execution_time,
                memory_used: result.memory_used,
                syscall_count: result.syscall_count,
            },
        })
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        let service = self.service.read().await;
        
        // Get region config
        let (sgx_config, sev_config) = service
            .regions
            .get(&self.region_id)
            .ok_or_else(|| {
                TeeError::VerificationError(format!("Region {} not found", self.region_id))
            })?
            .clone();

        Ok(TeeConfig {
            min_attestations: 1,
            verify_measurements: true,
        })
    }

    async fn update_config(&mut self, _new_config: TeeConfig) -> Result<(), TeeError> {
        // Config is currently fixed
        Ok(())
    }
}
