use tee_interface::{
    TeeController,
    ExecutionResult,
    TeeError,
    Result,
};
use borsh::{BorshSerialize, BorshDeserialize};
use std::io::Write;

#[derive(Debug, Clone)]
pub struct ExecutionInput {
    wasm_bytes: Vec<u8>,
    function: String,
    args: Vec<u8>,
}

impl BorshSerialize for ExecutionInput {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.wasm_bytes, writer)?;
        BorshSerialize::serialize(&self.function, writer)?;
        BorshSerialize::serialize(&self.args, writer)
    }
}

impl BorshDeserialize for ExecutionInput {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        Ok(Self {
            wasm_bytes: BorshDeserialize::deserialize(buf)?,
            function: BorshDeserialize::deserialize(buf)?,
            args: BorshDeserialize::deserialize(buf)?,
        })
    }

    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut slice: &[u8] = &buf;
        Self::deserialize(&mut slice)
    }
}

pub struct HyperExecutor {
    controller: Box<dyn TeeController>,
}

impl HyperExecutor {
    pub fn new(controller: Box<dyn TeeController>) -> Self {
        Self { controller }
    }

    pub async fn execute_wasm(
        &self,
        region_id: String,
        wasm_bytes: Vec<u8>,
        function: String,
        args: Vec<u8>,
        tx_id: Vec<u8>,
    ) -> Result<ExecutionResult> {
        // Prepare WASM input
        let input = self.prepare_wasm_input(wasm_bytes, function, args)?;
        
        // Execute with controller
        let mut result = self.controller.execute(region_id.clone(), input, true).await?;
        
        // Set transaction ID in result
        result.tx_id = tx_id;
        
        Ok(result)
    }

    pub async fn health_check(&self) -> Result<bool> {
        self.controller.health_check().await
    }

    fn prepare_wasm_input(
        &self,
        wasm_bytes: Vec<u8>,
        function: String,
        args: Vec<u8>,
    ) -> Result<Vec<u8>> {
        // Create execution input
        let input = ExecutionInput {
            wasm_bytes,
            function,
            args,
        };
        
        // Serialize input
        let mut buf = Vec::new();
        input.serialize(&mut buf).map_err(|e| TeeError::ExecutionError(format!("Failed to serialize input: {}", e)))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::sleep;
    use rand::Rng;
    use async_trait::async_trait;
    use tee_interface::{TeeAttestation, TeeType, PlatformMeasurement};
    use futures::future::join_all;

    #[tokio::test]
    async fn test_serialization() {
        let input = ExecutionInput {
            wasm_bytes: vec![1, 2, 3],
            function: "test".to_string(),
            args: vec![4, 5, 6],
        };

        // Test serialization
        let mut buf = Vec::new();
        input.serialize(&mut buf).unwrap();

        // Test deserialization
        let mut slice: &[u8] = &buf;
        let decoded = ExecutionInput::deserialize(&mut slice).unwrap();

        assert_eq!(decoded.wasm_bytes, input.wasm_bytes);
        assert_eq!(decoded.function, input.function);
        assert_eq!(decoded.args, input.args);
    }

    #[tokio::test]
    async fn test_execute_wasm() {
        let controller = MockController::new(None);
        let executor = HyperExecutor::new(Box::new(controller));

        let result = executor
            .execute_wasm(
                "test-region".to_string(),
                vec![1, 2, 3],
                "test_function".to_string(),
                vec![4, 5, 6],
                vec![7, 8, 9],
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_network_errors() {
        let mut config = MockConfig::default();
        config.network_error_rate = 1.0; // Always fail with network error
        let controller = MockController::new(Some(config));
        let executor = HyperExecutor::new(Box::new(controller));

        let result = executor
            .execute_wasm(
                "test-region".to_string(),
                vec![1, 2, 3],
                "test_function".to_string(),
                vec![4, 5, 6],
                vec![7, 8, 9],
            )
            .await;

        assert!(matches!(result, Err(TeeError::NetworkError(_))));
    }

    #[tokio::test]
    async fn test_region_health() {
        let controller = MockController::new(None);
        {
            let mut regions = controller.region_states.lock().unwrap();
            regions.insert("unhealthy-region".to_string(), false);
        }

        let executor = HyperExecutor::new(Box::new(controller));

        let result = executor
            .execute_wasm(
                "unhealthy-region".to_string(),
                vec![1, 2, 3],
                "test_function".to_string(),
                vec![4, 5, 6],
                vec![7, 8, 9],
            )
            .await;

        assert!(matches!(result, Err(TeeError::RegionError(_))));
    }

    #[tokio::test]
    async fn test_input_size_limit() {
        let mut config = MockConfig::default();
        config.max_input_size = 10; // Small limit for testing
        let controller = MockController::new(Some(config));
        let executor = HyperExecutor::new(Box::new(controller));

        let result = executor
            .execute_wasm(
                "test-region".to_string(),
                vec![0; 100], // Exceeds limit
                "test_function".to_string(),
                vec![4, 5, 6],
                vec![7, 8, 9],
            )
            .await;

        assert!(matches!(result, Err(TeeError::ExecutionError(_))));
    }

    #[tokio::test]
    async fn test_memory_limit() {
        let mut config = MockConfig::default();
        config.memory_limit = 50; // Small memory limit for testing
        let controller = MockController::new(Some(config));
        let executor = HyperExecutor::new(Box::new(controller));

        // First execution should succeed
        let result = executor
            .execute_wasm(
                "test-region".to_string(),
                vec![0; 20],
                "test_function".to_string(),
                vec![4, 5, 6],
                vec![7, 8, 9],
            )
            .await;
        assert!(result.is_ok());

        // Second execution should fail due to memory limit
        let result = executor
            .execute_wasm(
                "test-region".to_string(),
                vec![0; 40],
                "test_function".to_string(),
                vec![4, 5, 6],
                vec![7, 8, 9],
            )
            .await;
        assert!(matches!(result, Err(TeeError::ExecutionError(msg)) if msg.contains("memory")));
    }

    #[tokio::test]
    async fn test_health_check() {
        let controller = MockController::new(None);
        let executor = HyperExecutor::new(Box::new(controller));
        
        let result = executor.health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_concurrent_execution() {
        let mut config = MockConfig::default();
        config.network_error_rate = 0.0; // Disable network errors for this test
        let controller = MockController::new(Some(config));
        let executor = Arc::new(HyperExecutor::new(Box::new(controller)));
        
        // Create multiple concurrent execution tasks
        let mut handles = Vec::new();
        for i in 0..10 {
            let executor = executor.clone();
            let handle = tokio::spawn(async move {
                executor
                    .execute_wasm(
                        format!("region-{}", i),
                        vec![i as u8; 10],
                        format!("function-{}", i),
                        vec![4, 5, 6],
                        vec![7, 8, 9],
                    )
                    .await
            });
            handles.push(handle);
        }

        // Wait for all executions to complete
        let results = join_all(handles).await;
        
        // Verify results
        for result in results {
            let execution_result = result.unwrap();
            assert!(execution_result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_concurrent_memory_limits() {
        let mut config = MockConfig::default();
        config.memory_limit = 100; // Small memory limit
        config.network_error_rate = 0.0; // Disable network errors for this test
        let controller = MockController::new(Some(config));
        let executor = Arc::new(HyperExecutor::new(Box::new(controller)));
        
        // Create concurrent tasks that will exceed memory limit
        let mut handles = Vec::new();
        for i in 0..5 {
            let executor = executor.clone();
            let handle = tokio::spawn(async move {
                executor
                    .execute_wasm(
                        format!("region-{}", i),
                        vec![0; 30], // Each execution uses 30 bytes
                        format!("function-{}", i),
                        vec![4, 5, 6],
                        vec![7, 8, 9],
                    )
                    .await
            });
            handles.push(handle);
        }

        // Wait for all executions to complete
        let results = join_all(handles).await;
        
        // Count successes and failures
        let mut successes = 0;
        let mut failures = 0;
        
        for result in results {
            match result.unwrap() {
                Ok(_) => successes += 1,
                Err(TeeError::ExecutionError(msg)) if msg.contains("memory") => failures += 1,
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
        
        // Some executions should succeed, others should fail due to memory limit
        assert!(successes > 0, "Expected some successful executions");
        assert!(failures > 0, "Expected some failed executions due to memory limit");
        assert_eq!(successes + failures, 5);
    }

    #[tokio::test]
    async fn test_concurrent_region_health() {
        let controller = MockController::new(None);
        {
            let mut regions = controller.region_states.lock().unwrap();
            // Mark some regions as unhealthy
            regions.insert("region-1".to_string(), false);
            regions.insert("region-3".to_string(), false);
        }
        
        let executor = Arc::new(HyperExecutor::new(Box::new(controller)));
        
        // Create concurrent tasks for different regions
        let mut handles = Vec::new();
        for i in 0..5 {
            let executor = executor.clone();
            let handle = tokio::spawn(async move {
                executor
                    .execute_wasm(
                        format!("region-{}", i),
                        vec![i as u8; 10],
                        format!("function-{}", i),
                        vec![4, 5, 6],
                        vec![7, 8, 9],
                    )
                    .await
            });
            handles.push(handle);
        }

        // Wait for all executions to complete
        let results = join_all(handles).await;
        
        // Verify results
        let mut healthy_success = 0;
        let mut unhealthy_failure = 0;
        
        for (i, result) in results.into_iter().enumerate() {
            let region = format!("region-{}", i);
            match (region.as_str(), result.unwrap()) {
                ("region-1" | "region-3", Err(TeeError::RegionError(_))) => unhealthy_failure += 1,
                ("region-1" | "region-3", _) => panic!("Expected failure for unhealthy region"),
                (_, Ok(_)) => healthy_success += 1,
                (_, Err(e)) => panic!("Unexpected error for healthy region: {:?}", e),
            }
        }
        
        assert_eq!(healthy_success, 3, "Expected 3 successful executions");
        assert_eq!(unhealthy_failure, 2, "Expected 2 failed executions");
    }

    #[derive(Debug, Clone)]
    struct ExecutionStats {
        total_executions: usize,
        failed_executions: usize,
        last_execution_time: u64,
        total_bytes_processed: usize,
    }

    #[derive(Debug)]
    struct MockConfig {
        delay_ms: u64,
        error_rate: f64,
        max_input_size: usize,
        memory_limit: usize,
        network_error_rate: f64,
    }

    impl Default for MockConfig {
        fn default() -> Self {
            Self {
                delay_ms: 100,
                error_rate: 0.1,
                max_input_size: 1024 * 1024, // 1MB
                memory_limit: 1024 * 1024 * 10, // 10MB
                network_error_rate: 0.05,
            }
        }
    }

    struct MockController {
        config: Arc<Mutex<MockConfig>>,
        stats: Arc<Mutex<ExecutionStats>>,
        execution_count: AtomicUsize,
        region_states: Arc<Mutex<HashMap<String, bool>>>, // track region health
    }

    impl MockController {
        fn new(config: Option<MockConfig>) -> Self {
            let config = Arc::new(Mutex::new(config.unwrap_or_default()));
            Self {
                config,
                stats: Arc::new(Mutex::new(ExecutionStats {
                    total_executions: 0,
                    failed_executions: 0,
                    last_execution_time: 0,
                    total_bytes_processed: 0,
                })),
                execution_count: AtomicUsize::new(0),
                region_states: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        async fn simulate_network_delay(&self) -> Result<()> {
            let delay = self.config.lock().unwrap().delay_ms;
            sleep(Duration::from_millis(delay)).await;
            Ok(())
        }

        fn should_fail(&self) -> bool {
            let config = self.config.lock().unwrap();
            rand::thread_rng().gen::<f64>() < config.error_rate
        }

        fn should_simulate_network_error(&self) -> bool {
            let config = self.config.lock().unwrap();
            rand::thread_rng().gen::<f64>() < config.network_error_rate
        }

        fn validate_input(&self, input: &[u8]) -> Result<()> {
            let config = self.config.lock().unwrap();
            if input.len() > config.max_input_size {
                return Err(TeeError::ExecutionError(
                    format!("Input size {} exceeds maximum allowed size {}", 
                        input.len(), config.max_input_size)
                ));
            }
            Ok(())
        }

        fn validate_memory_usage(&self, input: &[u8]) -> Result<()> {
            let config = self.config.lock().unwrap();
            let stats = self.stats.lock().unwrap();
            
            // Calculate total memory usage including the new input
            let total_memory = stats.total_bytes_processed + input.len();
            
            if total_memory > config.memory_limit {
                return Err(TeeError::ExecutionError(
                    format!("Total memory usage {} would exceed limit {}", 
                        total_memory, config.memory_limit)
                ));
            }
            Ok(())
        }

        fn update_stats(&self, success: bool, input_size: usize) {
            let mut stats = self.stats.lock().unwrap();
            stats.total_executions += 1;
            if !success {
                stats.failed_executions += 1;
            }
            stats.last_execution_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            stats.total_bytes_processed += input_size;
        }

        fn is_region_healthy(&self, region_id: &str) -> bool {
            self.region_states
                .lock()
                .unwrap()
                .get(region_id)
                .copied()
                .unwrap_or(true)
        }
    }

    #[async_trait]
    impl TeeController for MockController {
        async fn execute(
            &self,
            region_id: String,
            input: Vec<u8>,
            attestation_required: bool,
        ) -> Result<ExecutionResult> {
            // Increment execution count
            self.execution_count.fetch_add(1, Ordering::SeqCst);

            // Simulate network delay
            self.simulate_network_delay().await?;

            // Simulate network errors
            if self.should_simulate_network_error() {
                self.update_stats(false, input.len());
                return Err(TeeError::NetworkError("Simulated network error".into()));
            }

            // Validate input size
            self.validate_input(&input)?;

            // Validate memory usage
            self.validate_memory_usage(&input)?;

            // Check region health
            if !self.is_region_healthy(&region_id) {
                self.update_stats(false, input.len());
                return Err(TeeError::RegionError(format!("Region {} is unhealthy", region_id)));
            }

            // Simulate random failures
            if self.should_fail() {
                self.update_stats(false, input.len());
                return Err(TeeError::ExecutionError("Simulated random failure".into()));
            }

            // Create attestation if required
            let attestations = if attestation_required {
                [
                    TeeAttestation {
                        tee_type: TeeType::Sgx,
                        measurement: PlatformMeasurement::Sgx {
                            mrenclave: [1u8; 32], // Simulated unique measurement
                            mrsigner: [2u8; 32],
                            miscselect: 0,
                            attributes: [0u8; 16],
                        },
                        signature: vec![3u8; 64], // Simulated signature
                    },
                    TeeAttestation {
                        tee_type: TeeType::Sev,
                        measurement: PlatformMeasurement::Sev {
                            measurement: [4u8; 32],
                            platform_info: [5u8; 32],
                            launch_digest: [6u8; 32],
                        },
                        signature: vec![7u8; 64],
                    },
                ]
            } else {
                [
                    TeeAttestation {
                        tee_type: TeeType::Sgx,
                        measurement: PlatformMeasurement::Sgx {
                            mrenclave: [0u8; 32],
                            mrsigner: [0u8; 32],
                            miscselect: 0,
                            attributes: [0u8; 16],
                        },
                        signature: vec![],
                    },
                    TeeAttestation {
                        tee_type: TeeType::Sev,
                        measurement: PlatformMeasurement::Sev {
                            measurement: [0u8; 32],
                            platform_info: [0u8; 32],
                            launch_digest: [0u8; 32],
                        },
                        signature: vec![],
                    },
                ]
            };

            // Update stats for successful execution
            self.update_stats(true, input.len());

            Ok(ExecutionResult {
                tx_id: vec![],
                state_hash: [0u8; 32],
                output: input,
                attestations,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                region_id,
            })
        }

        async fn health_check(&self) -> Result<bool> {
            // Simulate network delay
            self.simulate_network_delay().await?;

            // Simulate network errors
            if self.should_simulate_network_error() {
                return Err(TeeError::NetworkError("Simulated network error".into()));
            }

            Ok(true)
        }
    }
}
