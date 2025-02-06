use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tee_interface::{TeeExecutor, ExecutionPayload};
use crate::proto::teeservice::tee_execution_server::TeeExecution;
use crate::proto::teeservice::{
    ExecutionRequest,
    ExecutionResult,
    GetRegionsRequest,
    GetRegionsResponse,
    GetAttestationsRequest,
    RegionAttestations,
    CreateContractRequest,
    CreateContractResponse,
};

pub struct TeeServer {
    executor: Arc<Box<dyn TeeExecutor + Send + Sync>>,
}

impl TeeServer {
    pub fn new(executor: Box<dyn TeeExecutor + Send + Sync>) -> Self {
        Self {
            executor: Arc::new(executor),
        }
    }
}

pub struct TeeExecutionWrapper {
    inner: Arc<RwLock<TeeServer>>,
}

impl TeeExecutionWrapper {
    pub fn new(inner: Arc<RwLock<TeeServer>>) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl TeeExecution for TeeExecutionWrapper {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<ExecutionResult>, Status> {
        let server = self.inner.read().await;
        server.execute(request).await
    }

    async fn get_regions(
        &self,
        request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        let server = self.inner.read().await;
        server.get_regions(request).await
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        let server = self.inner.read().await;
        server.get_attestations(request).await
    }

    async fn create_contract(
        &self,
        request: Request<CreateContractRequest>,
    ) -> Result<Response<CreateContractResponse>, Status> {
        let server = self.inner.read().await;
        server.create_contract(request).await
    }
}

#[tonic::async_trait]
impl TeeExecution for TeeServer {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<ExecutionResult>, Status> {
        let req = request.into_inner();
        
        let payload = ExecutionPayload {
            params: req.clone().into(),
            input: req.parameters,
        };

        let result = self.executor
            .execute(&payload)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ExecutionResult {
            result: result.result,
            state_hash: result.state_hash,
            execution_time: result.stats.execution_time,
            memory_used: result.stats.memory_used,
            syscall_count: result.stats.syscall_count,
            attestations: result.attestations.iter().map(|a| a.clone().into()).collect(),
            timestamp: result.timestamp,
        }))
    }

    async fn get_regions(
        &self,
        _request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        let regions = self.executor
            .get_regions()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetRegionsResponse {
            regions: regions.into_iter().map(|r| r.into()).collect(),
        }))
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        let req = request.into_inner();
        let attestations = self.executor
            .get_attestations(&req.region_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RegionAttestations {
            attestations: attestations.into_iter().map(|a| a.into()).collect(),
        }))
    }

    async fn create_contract(
        &self,
        request: Request<CreateContractRequest>,
    ) -> Result<Response<CreateContractResponse>, Status> {
        let req = request.into_inner();
        let address = self.executor
            .deploy_contract(&req.wasm_code, &req.region_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let state_hash = self.executor
            .get_state_hash(&address)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateContractResponse {
            address,
            state_hash,
            timestamp: chrono::Utc::now().to_rfc3339(),
            attestations: vec![],
        }))
    }
}
