use tee_interface::prelude::*;
use async_trait::async_trait;
use thiserror::Error;
use reqwest;
use tokio_retry::{
    Retry,
    strategy::ExponentialBackoff,
};
use std::time::Duration;
use metrics::{counter, gauge};
use tracing::{info, error, instrument};
use std::fmt::Debug;
use hex;
use borsh::BorshDeserialize;

const METRIC_PREFIX: &str = "enarx_controller";

#[derive(Debug, Clone)]
pub struct EnarxConfig {
    /// Base URL for Enarx API
    pub api_url: String,
    /// Authentication token for Enarx API
    pub auth_token: Option<String>,
    /// Region configuration
    pub regions: Vec<RegionConfig>,
    /// Maximum number of retries for API calls
    pub max_retries: u32,
    /// Initial retry delay in milliseconds
    pub initial_retry_delay_ms: u64,
    /// Maximum retry delay in milliseconds
    pub max_retry_delay_ms: u64,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
}

impl Default for EnarxConfig {
    fn default() -> Self {
        Self {
            api_url: String::new(),
            auth_token: None,
            regions: Vec::new(),
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            request_timeout_ms: 30000,  // 30 seconds
            connect_timeout_ms: 10000,  // 10 seconds
        }
    }
}

/// Region configuration
#[derive(Debug, Clone)]
pub struct RegionConfig {
    /// Region identifier
    pub id: String,
    /// Region endpoint
    pub endpoint: String,
}

/// Errors specific to Enarx controller
#[derive(Error, Debug)]
pub enum ControllerError {
    #[error("Failed to initialize controller: {0}")]
    InitializationError(String),

    #[error("Failed to execute code: {0}")]
    ExecutionError(String),

    #[error("Failed to generate attestation: {0}")]
    AttestationError(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Request error: {0}")]
    RequestError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}

impl From<ControllerError> for TeeError {
    fn from(err: ControllerError) -> Self {
        match err {
            ControllerError::InitializationError(msg) => TeeError::InitializationError(msg),
            ControllerError::ExecutionError(msg) => TeeError::ExecutionError(msg),
            ControllerError::AttestationError(msg) => TeeError::AttestationError(msg),
            ControllerError::AuthenticationError(msg) => TeeError::InitializationError(msg),
            ControllerError::ApiError(msg) => TeeError::ExecutionError(msg),
            ControllerError::ConfigurationError(msg) => TeeError::InitializationError(msg),
            ControllerError::RequestError(msg) => TeeError::ExecutionError(msg),
            ControllerError::DeserializationError(msg) => TeeError::ExecutionError(msg),
        }
    }
}

impl EnarxConfig {
    /// Validates the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate max_retries
        if self.max_retries == 0 {
            return Err(ControllerError::ConfigurationError(
                "max_retries must be greater than 0".to_string()
            ).into());
        }
        if self.max_retries > 10 {
            return Err(ControllerError::ConfigurationError(
                "max_retries must not exceed 10".to_string()
            ).into());
        }

        // Validate retry delays
        if self.initial_retry_delay_ms == 0 {
            return Err(ControllerError::ConfigurationError(
                "initial_retry_delay_ms must be greater than 0".to_string()
            ).into());
        }
        if self.max_retry_delay_ms == 0 {
            return Err(ControllerError::ConfigurationError(
                "max_retry_delay_ms must be greater than 0".to_string()
            ).into());
        }
        if self.initial_retry_delay_ms > self.max_retry_delay_ms {
            return Err(ControllerError::ConfigurationError(
                "initial_retry_delay_ms must not exceed max_retry_delay_ms".to_string()
            ).into());
        }
        if self.max_retry_delay_ms > 30000 {
            return Err(ControllerError::ConfigurationError(
                "max_retry_delay_ms must not exceed 30 seconds".to_string()
            ).into());
        }

        // Validate timeouts
        if self.request_timeout_ms == 0 {
            return Err(ControllerError::ConfigurationError(
                "request_timeout_ms must be greater than 0".to_string()
            ).into());
        }
        if self.connect_timeout_ms == 0 {
            return Err(ControllerError::ConfigurationError(
                "connect_timeout_ms must be greater than 0".to_string()
            ).into());
        }
        if self.connect_timeout_ms > self.request_timeout_ms {
            return Err(ControllerError::ConfigurationError(
                "connect_timeout_ms must not exceed request_timeout_ms".to_string()
            ).into());
        }
        if self.request_timeout_ms > 300000 {
            return Err(ControllerError::ConfigurationError(
                "request_timeout_ms must not exceed 5 minutes".to_string()
            ).into());
        }

        Ok(())
    }
}

#[derive(Clone)]
struct RetryStrategy {
    backoff: ExponentialBackoff,
    max_retries: usize,
}

impl RetryStrategy {
    fn new(initial_delay_ms: u64, max_delay_ms: u64, max_retries: usize) -> Self {
        let backoff = ExponentialBackoff::from_millis(initial_delay_ms)
            .max_delay(Duration::from_millis(max_delay_ms));
        Self {
            backoff,
            max_retries,
        }
    }
}

impl Iterator for RetryStrategy {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        if self.max_retries == 0 {
            None
        } else {
            self.max_retries -= 1;
            Some(self.backoff.next().unwrap_or_else(|| Duration::from_millis(0)))
        }
    }
}

pub struct EnarxController {
    config: EnarxConfig,
    client: reqwest::Client,
}

impl EnarxController {
    pub fn new(config: EnarxConfig) -> Result<Self> {
        // Initialize metrics
        Self::init_metrics();
        
        // Validate configuration
        config.validate()?;
        
        // Create client with timeouts
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.request_timeout_ms))
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms))
            .build()
            .map_err(|e| ControllerError::InitializationError(
                format!("Failed to create HTTP client: {}", e)
            ))?;
            
        // Record initial metrics
        gauge!(
            format!("{}_regions_total", METRIC_PREFIX),
            config.regions.len() as f64
        );
        
        Ok(Self { 
            config,
            client,
        })
    }

    fn init_metrics() {
        // Initialize counters with 0
        counter!(format!("{}_api_calls_total", METRIC_PREFIX), 0);
        counter!(format!("{}_api_errors_total", METRIC_PREFIX), 0);
        counter!(format!("{}_retry_attempts_total", METRIC_PREFIX), 0);
        counter!(format!("{}_timeouts_total", METRIC_PREFIX), 0);
        
        // Initialize gauges
        gauge!(format!("{}_regions_total", METRIC_PREFIX), 0.0);
        gauge!(format!("{}_healthy_regions_total", METRIC_PREFIX), 0.0);
    }

    fn get_region_config(&self, region_id: &str) -> Result<&RegionConfig> {
        self.config.regions
            .iter()
            .find(|r| r.id == region_id)
            .ok_or_else(|| TeeError::ExecutionError(format!("Region {} not found", region_id)))
    }

    fn get_auth_header(&self) -> Result<String> {
        self.config.auth_token.as_ref()
            .ok_or_else(|| ControllerError::AuthenticationError("Authentication token not configured".to_string()).into())
            .map(|token| format!("Bearer {}", token))
    }

    fn create_retry_strategy(&self) -> RetryStrategy {
        RetryStrategy::new(
            self.config.initial_retry_delay_ms,
            self.config.max_retry_delay_ms,
            self.config.max_retries as usize
        )
    }

    fn create_mock_attestation(&self, tee_type: TeeType) -> TeeAttestation {
        match tee_type {
            TeeType::Sgx => TeeAttestation {
                tee_type,
                measurement: PlatformMeasurement::Sgx {
                    mrenclave: [0u8; 32],
                    mrsigner: [0u8; 32],
                    miscselect: 0,
                    attributes: [0u8; 16],
                },
                signature: vec![],
            },
            TeeType::Sev => TeeAttestation {
                tee_type,
                measurement: PlatformMeasurement::Sev {
                    measurement: [0u8; 32],
                    platform_info: [0u8; 32],
                    launch_digest: [0u8; 32],
                },
                signature: vec![],
            },
        }
    }

    #[instrument(skip(self, operation))]
    async fn execute_with_retry<F, Fut, T>(&self, operation: F, _region_id: Option<&str>) -> Result<T>
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = reqwest::Result<T>> + Send,
        T: Send + 'static,
    {
        let retry_strategy = self.create_retry_strategy();
        
        let result = Retry::spawn(retry_strategy, operation)
            .await
            .map_err(|_err| TeeError::ExecutionError("Failed to execute in Enarx".to_string()))?;
            
        Ok(result)
    }

    #[instrument(skip(self))]
    async fn execute_in_region(
        &self,
        region_config: &RegionConfig,
        input: Vec<u8>,
        attestation_required: bool,
    ) -> Result<ExecutionResult> {
        let enarx_url = format!("{}/execute", region_config.endpoint);
        let auth_header = self.get_auth_header()?;
        let region_id = region_config.id.clone();
        
        // Clone values that need to be moved into the closure
        let client = self.client.clone();
        let url = enarx_url;
        let auth = auth_header;
        let input = input;

        let bytes = self.execute_with_retry(
            move || {
                let client = client.clone();
                let url = url.clone();
                let auth = auth.clone();
                let input = input.clone();
                
                async move {
                    let response = client
                        .post(&url)
                        .header("Authorization", &auth)
                        .body(input)
                        .send()
                        .await?;

                    if !response.status().is_success() {
                        return Err(response.error_for_status().unwrap_err());
                    }

                    Ok(response.bytes().await?.to_vec())
                }
            },
            Some(&region_id),
        ).await.map_err(|_err| TeeError::ExecutionError("Failed to execute in Enarx".to_string()))?;

        // Create mock attestations when required
        let attestations = [
            self.create_mock_attestation(TeeType::Sgx),
            self.create_mock_attestation(TeeType::Sev),
        ];

        Ok(ExecutionResult {
            tx_id: vec![],
            state_hash: [0u8; 32],
            output: bytes,
            attestations,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            region_id,
        })
    }

    #[instrument(skip(self))]
    async fn health_check(&self) -> Result<bool> {
        if self.config.api_url.is_empty() {
            return Err(TeeError::InitializationError("API URL not configured".to_string()));
        }

        let auth_header = self.get_auth_header()?;
        info!("Starting health check for all regions");

        let mut healthy_regions = 0;

        for region in &self.config.regions {
            if region.endpoint.is_empty() {
                error!(
                    region_id = region.id,
                    "Region endpoint not configured"
                );
                return Err(TeeError::InitializationError(
                    format!("Endpoint not configured for region {}", region.id)
                ));
            }
            
            info!(region_id = region.id, "Checking region health");

            let health_url = format!("{}/health", region.endpoint);
            let auth_header_clone = auth_header.clone();
            let health_url_clone = health_url.clone();

            let response = self.execute_with_retry(
                move || {
                    let client = reqwest::Client::new();
                    client.get(&health_url_clone)
                        .header("Authorization", &auth_header_clone)
                        .send()
                },
                Some(&region.id)
            ).await.map_err(|_err| TeeError::ExecutionError("Failed to execute in Enarx".to_string()))?;

            if !response.status().is_success() {
                error!(
                    region_id = region.id,
                    status = response.status().as_u16(),
                    "Region is unhealthy"
                );
                return Err(TeeError::InitializationError(format!(
                    "Region {} is unhealthy: {}", 
                    region.id,
                    response.status()
                )));
            }

            healthy_regions += 1;
            info!(region_id = region.id, "Region is healthy");
        }

        gauge!(
            format!("{}_healthy_regions_total", METRIC_PREFIX),
            healthy_regions as f64
        );

        Ok(true)
    }

    async fn get_state(&self, region_id: String, contract_id: [u8; 32]) -> Result<ContractState> {
        let region_config = self.get_region_config(&region_id)?;
        let url = format!("{}/state/{}", region_config.endpoint, hex::encode(contract_id));
        let auth_header = self.get_auth_header()?;
        
        // Clone values that need to be moved into the closure
        let client = self.client.clone();
        let url = url;
        let auth = auth_header;
        
        let state_bytes = self.execute_with_retry(
            move || {
                let client = client.clone();
                let url = url.clone();
                let auth = auth.clone();
                
                async move {
                    let response = client
                        .get(&url)
                        .header("Authorization", &auth)
                        .send()
                        .await?;

                    if !response.status().is_success() {
                        return Err(response.error_for_status().unwrap_err());
                    }

                    Ok(response.bytes().await?.to_vec())
                }
            },
            Some(&region_id),
        ).await.map_err(|_err| TeeError::ExecutionError("Failed to get state from Enarx".to_string()))?;

        ContractState::deserialize(&mut &state_bytes[..])
            .map_err(|e| TeeError::ExecutionError(format!("Deserialization error: {}", e)))
    }
}

#[async_trait]
impl TeeController for EnarxController {
    async fn execute(
        &self,
        region_id: String,
        input: Vec<u8>,
        attestation_required: bool,
    ) -> Result<ExecutionResult> {
        let region_config = self.get_region_config(&region_id)?;
        self.execute_in_region(region_config, input, attestation_required).await
    }

    async fn health_check(&self) -> Result<bool> {
        self.health_check().await
    }

    async fn get_state(&self, region_id: String, contract_id: [u8; 32]) -> Result<ContractState> {
        self.get_state(region_id, contract_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> EnarxConfig {
        EnarxConfig {
            api_url: "http://localhost:8000".to_string(),
            auth_token: Some("test-token".to_string()),
            regions: vec![
                RegionConfig {
                    id: "us-east-1".to_string(),
                    endpoint: "http://localhost:8001".to_string(),
                },
                RegionConfig {
                    id: "us-west-1".to_string(),
                    endpoint: "http://localhost:8002".to_string(),
                },
            ],
            max_retries: 3,
            initial_retry_delay_ms: 10,
            max_retry_delay_ms: 100,
            request_timeout_ms: 1000,
            connect_timeout_ms: 500,
        }
    }

    #[tokio::test]
    async fn test_tee_execution() {
        let config = create_test_config();
        let controller = EnarxController::new(config).unwrap();

        let input = vec![1, 2, 3];
        let result = controller.execute(
            "us-east-1".to_string(),
            input.clone(),
            true,
        ).await.unwrap();

        assert_eq!(result.output, input);
        assert_eq!(result.attestations.len(), 2);
        assert_eq!(result.attestations[0].tee_type, TeeType::Sgx);
        assert_eq!(result.attestations[1].tee_type, TeeType::Sev);
        assert_eq!(result.region_id, "us-east-1");
    }

    #[tokio::test]
    async fn test_region_not_found() {
        let config = create_test_config();
        let controller = EnarxController::new(config).unwrap();

        let result = controller.execute(
            "invalid-region".to_string(),
            vec![1, 2, 3],
            true,
        ).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TeeError::ExecutionError(_)));
    }

    #[tokio::test]
    async fn test_health_check_with_empty_api_url() {
        let mut config = create_test_config();
        config.api_url = "".to_string();
        let controller = EnarxController::new(config).unwrap();

        let result = controller.health_check().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TeeError::InitializationError(_)));
    }

    #[tokio::test]
    async fn test_health_check_with_empty_endpoint() {
        let mut config = create_test_config();
        config.regions[0].endpoint = "".to_string();
        let controller = EnarxController::new(config).unwrap();

        let result = controller.health_check().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TeeError::InitializationError(_)));
    }

    #[tokio::test]
    async fn test_health_check_with_missing_auth() {
        let mut config = create_test_config();
        config.auth_token = None;
        let controller = EnarxController::new(config).unwrap();

        let result = controller.health_check().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TeeError::InitializationError(_)));
    }

    #[tokio::test]
    async fn test_invalid_timeout_config() {
        let mut config = create_test_config();
        config.connect_timeout_ms = 2000; // Greater than request timeout
        config.request_timeout_ms = 1000;
        
        let result = EnarxController::new(config);
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(matches!(e, TeeError::InitializationError(_)));
        }
    }

    #[tokio::test]
    async fn test_zero_timeout() {
        let mut config = create_test_config();
        config.request_timeout_ms = 0;
        
        let result = EnarxController::new(config);
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(matches!(e, TeeError::InitializationError(_)));
        }
    }

    #[tokio::test]
    async fn test_excessive_timeout() {
        let mut config = create_test_config();
        config.request_timeout_ms = 600000; // 10 minutes
        
        let result = EnarxController::new(config);
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(matches!(e, TeeError::InitializationError(_)));
        }
    }

    #[tokio::test]
    async fn test_execute_in_region() {
        let config = EnarxConfig {
            api_url: "http://localhost:8000".to_string(),
            auth_token: Some("test-token".to_string()),
            regions: vec![
                RegionConfig {
                    id: "us-east-1".to_string(),
                    endpoint: "http://localhost:8001".to_string(),
                },
            ],
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            request_timeout_ms: 30000,
            connect_timeout_ms: 10000,
        };

        let controller = EnarxController::new(config).unwrap();
        let input = vec![1, 2, 3, 4];
        let result = controller.execute_in_region(
            &controller.config.regions[0],
            input.clone(),
            true
        ).await.unwrap();

        assert_eq!(result.output, input);
        assert_eq!(result.attestations.len(), 2);
        assert_eq!(result.attestations[0].tee_type, TeeType::Sgx);
        assert_eq!(result.attestations[1].tee_type, TeeType::Sev);
        assert_eq!(result.region_id, "us-east-1");
    }

    #[tokio::test]
    async fn test_get_state() {
        let config = create_test_config();
        let controller = EnarxController::new(config).unwrap();
        
        // Create a test contract ID
        let mut contract_id = [0u8; 32];
        contract_id[0] = 1;  // Just to make it non-zero
        
        // Test getting state from a valid region
        let result = controller.get_state("us-east-1".to_string(), contract_id).await;
        assert!(result.is_ok(), "Should succeed for valid region");
        
        // Test getting state from an invalid region
        let result = controller.get_state("invalid-region".to_string(), contract_id).await;
        assert!(result.is_err(), "Should fail for invalid region");
        
        // Test error handling for network issues
        let mut bad_config = create_test_config();
        bad_config.regions[0].endpoint = "http://invalid-endpoint".to_string();
        let controller = EnarxController::new(bad_config).unwrap();
        
        let result = controller.get_state("us-east-1".to_string(), contract_id).await;
        assert!(result.is_err(), "Should fail for invalid endpoint");
    }
}