use std::process::Command;
use tee_interface::prelude::*;
use tonic::transport::Channel;
use crate::proto::teeservice::tee_execution_client::TeeExecutionClient;
use crate::proto::conversions::{to_interface_result, create_execution_request, create_get_attestations_request, to_interface_attestation};
use async_trait::async_trait;

#[derive(Default)]
pub struct EnarxController {
    vm_client: Option<TeeExecutionClient<Channel>>,
    endpoint: String,
}

impl EnarxController {
    pub fn new() -> Self {
        Self {
            vm_client: None,
            endpoint: "http://[::1]:50052".to_string(),
        }
    }

    pub fn with_endpoint(endpoint: String) -> Self {
        Self {
            vm_client: None,
            endpoint,
        }
    }
}

#[async_trait]
impl TeeController for EnarxController {
    async fn init(&mut self) -> Result<(), TeeError> {
        // TODO: Re-enable Enarx service when available
        // let _output = Command::new("enarx")
        //     .arg("run")
        //     .arg("--wasmcfgfile")
        //     .arg("config.toml")
        //     .spawn()
        //     .map_err(|e| TeeError::InitializationError(e.to_string()))?;

        // Connect to the service
        let uri = self.endpoint.parse::<tonic::transport::Uri>()
            .map_err(|e| TeeError::InitializationError(e.to_string()))?;
        let channel = Channel::builder(uri)
            .connect()
            .await
            .map_err(|e| TeeError::InitializationError(e.to_string()))?;

        self.vm_client = Some(TeeExecutionClient::new(channel));
        Ok(())
    }

    async fn execute(&mut self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // TODO: Implement Enarx execution
        Ok(ExecutionResult {
            result: vec![],
            attestation: TeeAttestation {
                data: vec![],
                signature: vec![],
                enclave_id: [0; 32],
                measurement: vec![],
                region_proof: None,
                timestamp: 0,
                enclave_type: TeeType::SGX,
            },
            state_hash: vec![],
            stats: ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
            },
        })
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