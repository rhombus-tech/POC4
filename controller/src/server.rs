use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use tee_interface::prelude::*;
use crate::enarx::EnarxController;

// Import generated proto code
pub mod tee {
    tonic::include_proto!("teeservice");
}

use tee::{
    tee_execution_server::TeeExecution,
    ExecutionRequest, ExecutionResult as ProtoExecutionResult, GetRegionsRequest, GetRegionsResponse,
    GetAttestationsRequest, RegionAttestations, TeeAttestation as ProtoTeeAttestation,
    Region,
};

pub struct TeeExecutionService {
    sgx_controller: Arc<RwLock<EnarxController>>,
    sev_controller: Arc<RwLock<EnarxController>>,
}

#[tonic::async_trait]
impl TeeExecution for TeeExecutionService {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<ProtoExecutionResult>, Status> {
        let req = request.into_inner();

        // Create execution payload
        let payload = ExecutionPayload {
            execution_id: 1, // TODO: Generate unique ID
            input: req.parameters,
            params: ExecutionParams {
                expected_hash: None, // TODO: Add support for hash verification
                detailed_proof: req.detailed_proof,
            },
        };

        // Execute in TEE
        let controller = match req.region_id.as_str() {
            "sgx" => &self.sgx_controller,
            "sev" => &self.sev_controller,
            _ => return Err(Status::invalid_argument("Invalid region ID")),
        };

        let result = controller
            .write()
            .await
            .execute(&payload)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Convert attestation
        let attestation = ProtoTeeAttestation {
            enclave_id: result.attestation.enclave_id.to_vec(),
            measurement: result.attestation.measurement,
            timestamp: chrono::Utc::now().to_rfc3339(),
            data: result.attestation.data,
            signature: result.attestation.signature,
            region_proof: result.attestation.region_proof.unwrap_or_default(),
            enclave_type: match req.region_id.as_str() {
                "sgx" => "SGX".to_string(),
                "sev" => "SEV".to_string(),
                _ => "UNKNOWN".to_string(),
            },
        };

        Ok(Response::new(ProtoExecutionResult {
            timestamp: chrono::Utc::now().to_rfc3339(),
            attestations: vec![attestation],
            state_hash: result.state_hash,
            result: result.result,
            execution_time: result.stats.execution_time,
            memory_used: result.stats.memory_used,
            syscall_count: result.stats.syscall_count,
        }))
    }

    async fn get_regions(
        &self,
        _request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        let mut regions = Vec::new();

        // Add SGX region
        regions.push(Region {
            id: "sgx".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            worker_ids: vec!["sgx-1".to_string()],
            supported_tee_types: vec!["SGX".to_string()],
            max_tasks: 10,
        });

        // Add SEV region
        regions.push(Region {
            id: "sev".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            worker_ids: vec!["sev-1".to_string()],
            supported_tee_types: vec!["SEV".to_string()],
            max_tasks: 10,
        });

        Ok(Response::new(GetRegionsResponse { regions }))
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        let req = request.into_inner();

        // Get attestations from controller
        let controller = match req.region_id.as_str() {
            "sgx" => &self.sgx_controller,
            "sev" => &self.sev_controller,
            _ => return Err(Status::invalid_argument("Invalid region ID")),
        };

        let attestations = controller
            .read()
            .await
            .get_attestations()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Convert attestations
        let proto_attestations = attestations.into_iter().map(|att| ProtoTeeAttestation {
            enclave_id: att.enclave_id.to_vec(),
            measurement: att.measurement,
            timestamp: chrono::Utc::now().to_rfc3339(),
            data: att.data,
            signature: att.signature,
            region_proof: att.region_proof.unwrap_or_default(),
            enclave_type: match req.region_id.as_str() {
                "sgx" => "SGX".to_string(),
                "sev" => "SEV".to_string(),
                _ => "UNKNOWN".to_string(),
            },
        }).collect();

        Ok(Response::new(RegionAttestations {
            attestations: proto_attestations,
        }))
    }
}

impl TeeExecutionService {
    pub fn new(
        sgx_controller: Arc<RwLock<EnarxController>>,
        sev_controller: Arc<RwLock<EnarxController>>,
    ) -> Self {
        Self {
            sgx_controller,
            sev_controller,
        }
    }
}
