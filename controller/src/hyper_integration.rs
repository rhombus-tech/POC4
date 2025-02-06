use tonic::transport::Channel;
use crate::proto::teeservice::tee_execution_client::TeeExecutionClient;
use crate::proto::conversions::{to_interface_result, create_execution_request, create_get_attestations_request, to_interface_attestation};
use tee_interface::prelude::*;
use async_trait::async_trait;

/// Controller that integrates TEE execution with HyperSDK
#[derive(Default)]
pub struct HyperTeeController {
    vm_client: Option<TeeExecutionClient<Channel>>,
}

impl HyperTeeController {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TeeController for HyperTeeController {
    async fn init(&mut self) -> Result<(), TeeError> {
        // Connect to the Hyper service
        let channel = Channel::from_static("http://[::1]:50052")
            .connect()
            .await
            .map_err(|e| TeeError::InitializationError(e.to_string()))?;

        self.vm_client = Some(TeeExecutionClient::new(channel));
        Ok(())
    }

    async fn execute(&mut self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let client = self.vm_client.as_ref().ok_or_else(|| {
            TeeError::ExecutionError("VM client not initialized".into())
        })?;

        let request = create_execution_request(payload, "hyper");
        let response = client.clone()
            .execute(tonic::Request::new(request))
            .await
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?
            .into_inner();

        Ok(to_interface_result(response))
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        Ok(TeeConfig::default())
    }

    async fn update_config(&mut self, _new_config: TeeConfig) -> Result<(), TeeError> {
        Ok(())
    }

    async fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError> {
        let client = self.vm_client.as_ref().ok_or_else(|| {
            TeeError::ExecutionError("VM client not initialized".into())
        })?;

        let request = create_get_attestations_request();
        let response = client.clone()
            .get_attestations(tonic::Request::new(request))
            .await
            .map_err(|e| TeeError::ExecutionError(e.to_string()))?
            .into_inner();

        Ok(response.attestations.into_iter()
            .map(|att| to_interface_attestation(&att))
            .collect())
    }
}
