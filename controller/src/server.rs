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
    DeployContractRequest,
    DeployContractResponse,
};
use crate::proto::conversions::{
    to_proto_execution_result, 
    to_proto_attestation, 
    to_proto_region
};

pub struct TeeServer {
    executor: Arc<dyn TeeExecutor + Send + Sync>,
}

impl TeeServer {
    pub fn new(executor: Arc<dyn TeeExecutor + Send + Sync>) -> Self {
        Self { executor }
    }
    
    pub async fn serve(&self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", port).parse().unwrap();
        
        // Create a locked reference to self
        let server_arc = Arc::new(RwLock::new(TeeServer {
            executor: self.executor.clone(),
        }));
        
        // Create the gRPC service
        let svc = crate::proto::teeservice::tee_execution_server::TeeExecutionServer::new(
            TeeExecutionWrapper::new(server_arc)
        );
        
        // Start the server
        tonic::transport::Server::builder()
            .add_service(svc)
            .serve(addr)
            .await?;
        
        Ok(())
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

    async fn deploy_contract(
        &self,
        request: Request<DeployContractRequest>,
    ) -> Result<Response<DeployContractResponse>, Status> {
        // Clone Arc to avoid holding read lock across await points
        let inner_clone = self.inner.clone();
        
        // Use async block to avoid holding the lock across await points
        let deploy_future = async move {
            let guard = inner_clone.read().await;
            guard.deploy_contract(request).await
        };
        
        deploy_future.await
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
            region_id: None,
            target_tee: None,
            tee_type: None,
            allow_fallback: Some(true),
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
    
    async fn deploy_contract(
        &self,
        request: Request<DeployContractRequest>,
    ) -> Result<Response<DeployContractResponse>, Status> {
        let req = request.into_inner();
        
        let contract_id = self.executor
            .deploy_contract(&req.contract_bytes, &req.region_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
            
        // Get attestations for the deployed contract
        let attestations = self.executor
            .get_attestations(&req.region_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
            
        Ok(Response::new(DeployContractResponse {
            contract_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            attestations: attestations.iter().map(to_proto_attestation).collect(),
        }))
    }
}
