use tee_controller::HyperTeeController;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeError, ExecutionResult, ExecutionStats, TeeAttestation, RegionInfo};
use std::sync::{Arc, RwLock, atomic::{AtomicUsize, Ordering}};
use std::collections::HashMap;
use std::time::Duration;
use async_trait::async_trait;

/// Mock implementation of TeeExecutor for testing
#[derive(Clone)]
pub struct MockTeeExecutor {
    // Add internal state for the mock
    counter: Arc<AtomicUsize>,
    // Add a shared key-value store for persistence
    kv_store: Arc<RwLock<HashMap<String, String>>>,
}

impl MockTeeExecutor {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            kv_store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl TeeExecutor for MockTeeExecutor {
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Simulate some processing time
        tokio::time::sleep(Duration::from_millis(5)).await;
        
        // Increment the operation counter
        let op_count = self.counter.fetch_add(1, Ordering::SeqCst);
        
        // Parse input for add operation
        let input_str = String::from_utf8_lossy(&payload.input);
        
        // Create a simple mock result
        let result = ExecutionResult {
            result: format!("Mock execution result #{}", op_count).into_bytes(),
            gas_used: 1000,
            logs: vec![],
            stats: ExecutionStats {
                cpu_time_us: 1000,
                memory_used: 1024 * 1024,
                syscall_count: 10,
            }
        };
        
        Ok(result)
    }

    async fn verify_attestation(&self, _attestation: &TeeAttestation) -> Result<bool, TeeError> {
        Ok(true)
    }

    async fn get_region_info(&self) -> Result<RegionInfo, TeeError> {
        Ok(RegionInfo {
            region_id: "test-region".to_string(),
            location: "test-location".to_string(),
            provider: "test-provider".to_string(),
        })
    }
}

/// Creates a mock TEE executor for testing
pub fn create_mock_executor() -> Arc<MockTeeExecutor> {
    Arc::new(MockTeeExecutor::new())
}

/// Creates a test controller
pub async fn create_test_controller() -> Arc<HyperTeeController> {
    Arc::new(HyperTeeController::new().await)
}
