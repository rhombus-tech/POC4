use tonic::transport::Channel;
use tonic::Request;
use std::fmt::Debug;

/// Client for interacting with the TEE peer service
#[derive(Debug, Clone)]
pub struct TeeServiceClient {
    client: crate::proto::teeservice::tee_execution_client::TeeExecutionClient<Channel>,
}

impl TeeServiceClient {
    /// Create a new TEE service client
    pub async fn connect(dst: impl AsRef<str>) -> Result<Self, tonic::transport::Error> {
        let client = crate::proto::teeservice::tee_execution_client::TeeExecutionClient::connect(dst.as_ref().to_string()).await?;
        Ok(Self { client })
    }

    /// Execute a transaction on a peer TEE
    pub async fn execute(
        &mut self, 
        request: crate::proto::teeservice::ExecutionRequest
    ) -> Result<crate::proto::teeservice::ExecutionResult, tonic::Status> {
        let response = self.client.execute(Request::new(request)).await?;
        Ok(response.into_inner())
    }

    /// Get regions from the TEE service
    pub async fn get_regions(
        &mut self,
    ) -> Result<Vec<crate::proto::teeservice::Region>, tonic::Status> {
        let request = crate::proto::teeservice::GetRegionsRequest {};
        let response = self.client.get_regions(Request::new(request)).await?;
        let regions_response = response.into_inner();
        Ok(regions_response.regions)
    }

    /// Get attestations for a region
    pub async fn get_attestations(
        &mut self,
        region_id: &str,
    ) -> Result<Vec<crate::proto::teeservice::TeeAttestation>, tonic::Status> {
        let request = crate::proto::teeservice::GetAttestationsRequest {
            region_id: region_id.to_string(),
        };
        let response = self.client.get_attestations(Request::new(request)).await?;
        let attestations_response = response.into_inner();
        Ok(attestations_response.attestations)
    }
}
