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
