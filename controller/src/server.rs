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
};
use crate::proto::conversions::{
    to_proto_execution_result, 
    to_proto_attestation, 
    to_proto_region
};

pub struct TeeServer {
    executor: Box<dyn TeeExecutor + Send + Sync>,
}

impl TeeServer {
    pub fn new(executor: Box<dyn TeeExecutor + Send + Sync>) -> Self {
        Self { executor }
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
        // Clone Arc to avoid holding read lock across await points
        let inner_clone = self.inner.clone();
        
        // Use async block to avoid holding the lock across await points
        let execute_future = async move {
            let guard = inner_clone.read().await;
            guard.execute(request).await
        };
        
        execute_future.await
    }

    async fn get_regions(
        &self,
        request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        // Clone Arc to avoid holding read lock across await points
        let inner_clone = self.inner.clone();
        
        // Use async block to avoid holding the lock across await points
        let regions_future = async move {
            let guard = inner_clone.read().await;
            guard.get_regions(request).await
        };
        
        regions_future.await
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        // Clone Arc to avoid holding read lock across await points
        let inner_clone = self.inner.clone();
        
        // Use async block to avoid holding the lock across await points
        let attestations_future = async move {
            let guard = inner_clone.read().await;
            guard.get_attestations(request).await
        };
        
        attestations_future.await
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
            operation_id: None,
            previous_operation_id: None,
            operation_context: None,
        };

        let result = self.executor
            .execute(&payload)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(to_proto_execution_result(&result)))
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
            regions: regions.iter().map(to_proto_region).collect(),
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
            attestations: attestations.iter().map(to_proto_attestation).collect(),
        }))
    }
}
