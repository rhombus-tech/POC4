/*!
 * Protocol client implementation for cross-regional communication
 */

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, trace, warn};

use crate::{ClientConfig, error::{Result, Error, ProtocolError}};
use super::binary_protocol::{Message, MessageType, ParameterData, ParameterFormatType};
use super::tee_transport::{TeeTransport, TeeType, VerificationAccumulator};
use super::types::*;

/// Client for cross-regional protocol communication
pub struct ProtocolClient {
    config: Arc<ClientConfig>,
    /// Connection cache to avoid re-establishing connections
    connections: Mutex<HashMap<String, TeeTransport>>,
    /// Verification accumulators for cross-regional verification
    accumulators: Mutex<HashMap<String, VerificationAccumulator>>,
}

impl ProtocolClient {
    /// Create a new protocol client with the given configuration
    pub fn new(config: Arc<ClientConfig>) -> Self {
        Self { 
            config,
            connections: Mutex::new(HashMap::new()),
            accumulators: Mutex::new(HashMap::new()),
        }
    }
    
    /// Execute a transaction in a cross-regional context
    pub async fn execute_cross_region(
        &self,
        source_region: &str,
        target_region: &str,
        contract_id: &str,
        function: &str,
        parameters: Vec<u8>,
        parameter_format: ParameterFormat,
    ) -> Result<TransactionResponse> {
        // Verify that both regions are configured
        self.ensure_region_exists(source_region)?;
        self.ensure_region_exists(target_region)?;
        
        // Create transaction ID
        let id = self.generate_transaction_id();
        
        // Build the transaction request
        let request = TransactionRequest {
            id: id.clone(),
            source_region: source_region.to_string(),
            target_region: target_region.to_string(),
            contract_id: contract_id.to_string(),
            function: function.to_string(),
            parameters,
            parameter_format,
            timestamp: self.current_timestamp(),
            metadata: Default::default(),
        };
        
        // Execute the transaction (mocked for now)
        self.execute_transaction(request).await
    }
    
    /// Verify a transaction across multiple regions
    pub async fn verify_transaction(
        &self,
        transaction_id: &str,
        regions: Vec<String>,
    ) -> Result<VerificationResponse> {
        // Ensure all regions exist
        for region in &regions {
            self.ensure_region_exists(region)?;
        }
        
        // Build verification request
        let request = VerificationRequest {
            transaction_id: transaction_id.to_string(),
            regions,
            context: Default::default(),
        };
        
        // Perform verification (mocked for now)
        self.perform_verification(request).await
    }
    
    /// Execute a transaction (internal implementation)
    async fn execute_transaction(&self, request: TransactionRequest) -> Result<TransactionResponse> {
        let target_region = request.target_region.clone();
        let endpoint = self.config.regions.get(&target_region)
            .ok_or_else(|| ProtocolError::RegionNotFound(target_region.clone()))?
            .clone();
        
        // Get or create transport for this region
        let mut transport = {
            let mut connections = self.connections.lock().unwrap();
            match connections.get_mut(&target_region) {
                Some(transport) => transport.clone(),
                None => {
                    let tee_type = self.determine_tee_type(&target_region)?;
                    let mut new_transport = TeeTransport::new(endpoint, tee_type);
                    
                    // Connect and perform attestation
                    new_transport.connect().await?;
                    new_transport.attest().await?;
                    
                    connections.insert(target_region.clone(), new_transport.clone());
                    new_transport
                }
            }
        };
        
        // Convert parameter format
        let parameter_data = match request.parameter_format {
            ParameterFormat::LengthPrefixed => {
                ParameterData::to_length_prefixed(&request.parameters)
            },
            ParameterFormat::Direct => {
                ParameterData::to_direct(&request.parameters)
            }
        };
        
        // Build binary protocol request
        let binary_request = BinaryTransactionRequest {
            transaction_id: request.id.clone(),
            source_region: request.source_region.clone(),
            target_region: request.target_region.clone(),
            contract_id: request.contract_id.clone(),
            function: request.function.clone(),
            parameters: parameter_data,
            timestamp: request.timestamp,
        };
        
        // Send request and wait for response
        let timeout_duration = Duration::from_secs(10);
        let binary_response = match timeout(timeout_duration, transport.request::<BinaryTransactionRequest, BinaryTransactionResponse>(binary_request)).await {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => {
                error!("Transaction request failed: {}", e);
                return Err(e);
            },
            Err(_) => {
                error!("Transaction request timed out after {:?}", timeout_duration);
                return Err(ProtocolError::Timeout.into());
            }
        };
        
        // Update verification accumulator
        {
            let mut accumulators = self.accumulators.lock().unwrap();
            if !accumulators.contains_key(&target_region) {
                // Initialize new accumulator with first response
                let initial_value = match &binary_response.accumulator {
                    Some(acc) => {
                        let mut arr = [0u8; 32];
                        if acc.len() == 32 {
                            arr.copy_from_slice(acc);
                        } else {
                            warn!("Received invalid accumulator size: {}", acc.len());
                        }
                        arr
                    },
                    None => [0u8; 32]
                };
                
                accumulators.insert(target_region.clone(), VerificationAccumulator::new(initial_value));
            } else if let Some(acc) = &binary_response.accumulator {
                if acc.len() == 32 {
                    // Validate accumulator matches our expectation
                    let current_acc = accumulators.get(&target_region).unwrap();
                    let mut expected = [0u8; 32];
                    expected.copy_from_slice(acc);
                    
                    if current_acc.value() != expected {
                        warn!(
                            "Accumulator mismatch for region {}: expected {:?}, got {:?}",
                            target_region, current_acc.value(), expected
                        );
                    }
                }
            }
            
            // Update with latest transaction
            if let Some(acc) = accumulators.get_mut(&target_region) {
                acc.update(binary_response.result_data.as_slice());
            }
        }
        
        // Convert to public response type
        let execution_result = match binary_response.status {
            0 => ExecutionResult::Success,
            1 => ExecutionResult::Failure,
            2 => ExecutionResult::Rejected,
            _ => ExecutionResult::Unknown,
        };
        
        let attestation = binary_response.attestation.map(|att| AttestationData {
            tee_identity: att.tee_identity,
            report: att.report,
            signature: att.signature,
            public_key: att.public_key,
        });
        
        Ok(TransactionResponse {
            id: request.id,
            source_region: request.source_region,
            target_region: request.target_region,
            result: execution_result,
            data: binary_response.result_data,
            attestation,
            timestamp: binary_response.timestamp,
            metadata: Default::default(),
        })
    }
    
    /// Perform verification (internal implementation)
    async fn perform_verification(&self, request: VerificationRequest) -> Result<VerificationResponse> {
        let transaction_id = request.transaction_id.clone();
        let mut region_results = std::collections::HashMap::new();
        let mut overall_result = VerificationResult::Verified;

        // Perform verification for each region
        for region in &request.regions {
            let endpoint = match self.config.regions.get(region) {
                Some(ep) => ep.clone(),
                None => {
                    region_results.insert(
                        region.clone(),
                        RegionVerificationResult {
                            status: VerificationStatus::Failed,
                            details: Some(format!("Region not found: {}", region)),
                        },
                    );
                    overall_result = VerificationResult::Failed;
                    continue;
                }
            };
            
            // Get or create transport for this region
            let mut transport = {
                let mut connections = self.connections.lock().unwrap();
                match connections.get_mut(region) {
                    Some(transport) => transport.clone(),
                    None => {
                        let tee_type = self.determine_tee_type(region)?;
                        let mut new_transport = TeeTransport::new(endpoint, tee_type);
                        
                        // Connect and perform attestation
                        match new_transport.connect().await {
                            Ok(_) => (),
                            Err(e) => {
                                region_results.insert(
                                    region.clone(),
                                    RegionVerificationResult {
                                        status: VerificationStatus::Failed,
                                        details: Some(format!("Connection failed: {}", e)),
                                    },
                                );
                                overall_result = VerificationResult::Failed;
                                continue;
                            }
                        };
                        
                        match new_transport.attest().await {
                            Ok(_) => (),
                            Err(e) => {
                                region_results.insert(
                                    region.clone(),
                                    RegionVerificationResult {
                                        status: VerificationStatus::Failed,
                                        details: Some(format!("Attestation failed: {}", e)),
                                    },
                                );
                                overall_result = VerificationResult::Failed;
                                continue;
                            }
                        };
                        
                        connections.insert(region.clone(), new_transport.clone());
                        new_transport
                    }
                }
            };

            // Build binary protocol verification request
            let binary_request = BinaryVerificationRequest {
                transaction_id: transaction_id.clone(),
                regions: request.regions.clone(),
                context: vec![],
            };
            
            // Send request and wait for response
            let timeout_duration = Duration::from_secs(5);
            match timeout(timeout_duration, transport.request::<BinaryVerificationRequest, BinaryVerificationResponse>(binary_request)).await {
                Ok(Ok(response)) => {
                    // Check accumulator consistency
                    let acc_verified = match self.accumulators.lock().unwrap().get(region) {
                        Some(acc) => {
                            if let Some(remote_acc) = &response.accumulator {
                                if remote_acc.len() == 32 {
                                    let mut expected = [0u8; 32];
                                    expected.copy_from_slice(remote_acc);
                                    acc.value() == expected
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        },
                        None => true, // No local accumulator yet, accept remote
                    };

                    let status = if response.verified && acc_verified {
                        VerificationStatus::Success
                    } else if !response.verified {
                        VerificationStatus::Failed
                    } else {
                        VerificationStatus::AccumulatorMismatch
                    };

                    if status != VerificationStatus::Success {
                        overall_result = VerificationResult::Failed;
                    }

                    region_results.insert(
                        region.clone(),
                        RegionVerificationResult {
                            status,
                            details: response.details,
                        },
                    );
                },
                Ok(Err(e)) => {
                    error!("Verification request failed for {}: {}", region, e);
                    region_results.insert(
                        region.clone(),
                        RegionVerificationResult {
                            status: VerificationStatus::Failed,
                            details: Some(format!("Request failed: {}", e)),
                        },
                    );
                    overall_result = VerificationResult::Failed;
                },
                Err(_) => {
                    error!("Verification request timed out for {} after {:?}", region, timeout_duration);
                    region_results.insert(
                        region.clone(),
                        RegionVerificationResult {
                            status: VerificationStatus::Failed,
                            details: Some(format!("Request timed out after {:?}", timeout_duration)),
                        },
                    );
                    overall_result = VerificationResult::Failed;
                }
            }
        }
        
        Ok(VerificationResponse {
            transaction_id,
            result: overall_result,
            region_results,
            timestamp: self.current_timestamp(),
        })
    }
    
    /// Ensure a region exists in the configuration
    fn ensure_region_exists(&self, region: &str) -> Result<()> {
        if self.config.regions.contains_key(region) {
            Ok(())
        } else {
            Err(ProtocolError::RegionNotFound(region.to_string()).into())
        }
    }
    
    /// Generate a unique transaction ID
    fn generate_transaction_id(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        use rand::{thread_rng, Rng};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
            
        let random = thread_rng().gen::<u32>();
        format!("tx-{}-{:x}-{:x}", self.current_timestamp(), now & 0xFFFFFFFF, random)
    }
    
    /// Get the current timestamp in milliseconds
    fn current_timestamp(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    
    /// Determine the TEE type for a region
    fn determine_tee_type(&self, region: &str) -> Result<TeeType> {
        // In a production environment, this would look up the TEE type from configuration
        // or determine it from the endpoint characteristics
        
        // For now, we'll use a simple heuristic based on region name
        if region.contains("sgx") || region.contains("intel") {
            Ok(TeeType::IntelSgx)
        } else if region.contains("sev") || region.contains("amd") {
            Ok(TeeType::AmdSev)
        } else {
            // Default to Intel SGX for compatibility
            Ok(TeeType::IntelSgx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClientConfig;
    
    fn create_test_client() -> ProtocolClient {
        let config = ClientConfig {
            regions: [
                ("us-west".to_string(), "https://tee-us-west.example.com".to_string()),
                ("eu-central".to_string(), "https://tee-eu-central.example.com".to_string()),
            ].into(),
            attestation: None,
            external_apis: Default::default(),
        };
        
        ProtocolClient::new(Arc::new(config))
    }
    
    #[tokio::test]
    async fn test_execute_cross_region() {
        let client = create_test_client();
        
        let result = client.execute_cross_region(
            "us-west",
            "eu-central",
            "test-contract",
            "test-function",
            vec![1, 2, 3, 4],
            ParameterFormat::Direct,
        ).await;
        
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.result, ExecutionResult::Success);
        assert_eq!(response.source_region, "us-west");
        assert_eq!(response.target_region, "eu-central");
    }
    
    #[tokio::test]
    async fn test_unknown_region() {
        let client = create_test_client();
        
        let result = client.execute_cross_region(
            "us-west",
            "unknown-region",
            "test-contract",
            "test-function",
            vec![1, 2, 3, 4],
            ParameterFormat::Direct,
        ).await;
        
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::ClientError::Protocol(ProtocolError::RegionNotFound(region)) => {
                assert_eq!(region, "unknown-region");
            },
            err => panic!("Unexpected error: {:?}", err),
        }
    }
    
    #[tokio::test]
    async fn test_verify_transaction() {
        let client = create_test_client();
        
        // First execute a transaction
        let execute_result = client.execute_cross_region(
            "us-west",
            "eu-central",
            "test-contract",
            "test-function",
            vec![1, 2, 3, 4],
            ParameterFormat::Direct,
        ).await.unwrap();
        
        // Then verify it
        let verify_result = client.verify_transaction(
            &execute_result.id,
            vec!["us-west".to_string(), "eu-central".to_string()],
        ).await;
        
        assert!(verify_result.is_ok());
        let response = verify_result.unwrap();
        assert_eq!(response.result, VerificationResult::Verified);
        assert_eq!(response.region_results.len(), 2);
    }
}
