use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use crate::server::TeeExecutionService;
use crate::server::teeservice::tee_execution_server::TeeExecution;
use crate::server::teeservice::{
    ExecutionRequest, ExecutionResult,
    GetRegionsRequest, GetRegionsResponse,
    GetAttestationsRequest, RegionAttestations,
};

#[derive(Clone)]
pub struct TeeServiceWrapper {
    inner: Arc<RwLock<TeeExecutionService>>,
}

impl TeeServiceWrapper {
    pub fn new(service: Arc<RwLock<TeeExecutionService>>) -> Self {
        Self { inner: service }
    }
}

#[tonic::async_trait]
impl TeeExecution for TeeServiceWrapper {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<ExecutionResult>, Status> {
        self.inner.read().await.execute(request).await
    }

    async fn get_regions(
        &self,
        request: Request<GetRegionsRequest>,
    ) -> Result<Response<GetRegionsResponse>, Status> {
        self.inner.read().await.get_regions(request).await
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<RegionAttestations>, Status> {
        self.inner.read().await.get_attestations(request).await
    }
}
