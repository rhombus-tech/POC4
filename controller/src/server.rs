use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use chrono::Utc;
use tee_interface::prelude::*;
use super::paired_executor::TeeExecutorPair;
use super::proto::conversions::to_proto_result;

pub use super::proto::teeservice::{self, ExecutionRequest, ExecutionResult, GetRegionsRequest, GetRegionsResponse, GetAttestationsRequest, RegionAttestations, Region, TeeAttestation};
pub use self::teeservice::tee_execution_server::TeeExecution;

// Convert interface TeeAttestation to proto TeeAttestation
fn convert_attestation(att: &tee_interface::TeeAttestation) -> TeeAttestation {
    TeeAttestation {
        data: att.data.clone(),
        signature: att.signature.clone(),
        timestamp: Utc::now().to_rfc3339(),
        enclave_id: att.enclave_id.to_vec(),
        measurement: att.measurement.clone(),
        region_proof: att.region_proof.clone().unwrap_or_default(),
        enclave_type: "SGX".to_string(), // TODO: Make this dynamic
    }
}

#[derive(Default)]
pub struct TeeExecutionService {
    pub regions: HashMap<String, (String, String)>, // region_id -> (sgx_config, sev_config)
    executors: HashMap<String, Arc<TeeExecutorPair>>, // region_id -> executor pair
}

impl TeeExecutionService {
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
            executors: HashMap::new(),
        }
    }

    pub fn add_region(&mut self, region_id: String, sgx_config: String, sev_config: String) {
        self.regions.insert(region_id.clone(), (sgx_config.clone(), sev_config.clone()));
        let executor = Arc::new(TeeExecutorPair::new(region_id.clone(), sgx_config, sev_config));
        self.executors.insert(region_id, executor);
    }

    pub async fn init(&mut self) -> Result<(), TeeError> {
        for executor in self.executors.values() {
            executor.init().await?;
        }
        Ok(())
    }

    pub fn get_executor(&self, region_id: &str) -> Option<Arc<TeeExecutorPair>> {
        self.executors.get(region_id).cloned()
    }
}

#[async_trait::async_trait]
impl TeeExecution for TeeExecutionService {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<ExecutionResult>, Status> {
        let req = request.into_inner();
        println!("Received execution request for region: {}", req.region_id);
        println!("Function call: {}", req.function_call);
        println!("Parameters size: {}", req.parameters.len());
        
        // Get executor for region
        let executor = self.executors.get(&req.region_id)
            .ok_or_else(|| Status::not_found(format!("Region {} not found", req.region_id)))?;

        // Convert proto request to execution payload
        let payload = ExecutionPayload {
            execution_id: req.id_to.parse::<u64>()
                .map_err(|e| Status::invalid_argument(format!("Invalid execution ID: {}", e)))?,
            input: req.parameters,
            params: ExecutionParams {
                expected_hash: if req.expected_hash.is_empty() {
                    None
                } else {
                    // Convert Vec<u8> to [u8; 32]
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(&req.expected_hash);
                    Some(hash)
                },
                detailed_proof: req.detailed_proof,
                function_call: req.function_call,
            },
        };

        // Execute in paired TEEs
        let result = executor.execute(&payload)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        println!("SGX result: {:?}", result.sgx.result);
        println!("SEV result: {:?}", result.sev.result);

        // Convert result back to proto format using our conversion function
        let proto_result = to_proto_result(result.sgx);  // Use the SGX result

        println!("Final proto result: {:?}", proto_result.result);

        Ok(Response::new(proto_result))
    }

    async fn get_regions(
        &self,
        _request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        let regions = self.regions.iter().map(|(id, _configs)| {
            Region {
                id: id.clone(),
                created_at: Utc::now().to_rfc3339(),
                worker_ids: vec![],
                supported_tee_types: vec!["SGX".to_string(), "SEV".to_string()],
                max_tasks: 100,
            }
        }).collect();

        Ok(Response::new(GetRegionsResponse { regions }))
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        let req = request.into_inner();
        
        // Get executor for region
        let executor = self.executors.get(&req.region_id)
            .ok_or_else(|| Status::not_found(format!("Region {} not found", req.region_id)))?;

        // Get attestations from both TEEs
        let (sgx_att, sev_att) = executor.get_attestations()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RegionAttestations {
            attestations: vec![
                convert_attestation(&sgx_att),
                convert_attestation(&sev_att)
            ],
        }))
    }
}

pub struct TeeServiceWrapper {
    service: Arc<RwLock<TeeExecutionService>>,
}

impl TeeServiceWrapper {
    pub fn new(service: Arc<RwLock<TeeExecutionService>>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl TeeExecution for TeeServiceWrapper {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<ExecutionResult>, Status> {
        let service = self.service.read().await;
        service.execute(request).await
    }

    async fn get_regions(
        &self,
        request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        let service = self.service.read().await;
        service.get_regions(request).await
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        let service = self.service.read().await;
        service.get_attestations(request).await
    }
}
