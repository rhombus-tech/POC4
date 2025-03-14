use tee_controller::HyperTeeController;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeError, ExecutionResult, ExecutionStats, TeeAttestation, RegionInfo, TeeType};
use std::sync::{Arc, RwLock, atomic::{AtomicUsize, Ordering}};
use std::collections::HashMap;
use std::time::Duration;
use async_trait::async_trait;
use chrono;
use rand;

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
        let _input_str = String::from_utf8_lossy(&payload.input);
        
        // Create a simple mock result
        let result = ExecutionResult {
            result: format!("Mock execution result #{}", op_count).into_bytes(),
            state_hash: vec![0, 1, 2, 3],
            attestations: vec![TeeAttestation {
                enclave_id: vec![1, 2, 3],
                measurement: vec![4, 5, 6],
                timestamp: chrono::Utc::now().timestamp() as u64,
                data: vec![7, 8, 9],
                signature: vec![10, 11, 12],
                region_proof: None,
                enclave_type: TeeType::SGX,
            }],
            stats: ExecutionStats {
                execution_time: 1,
                memory_used: 1024 * 1024,
                syscall_count: 10,
                network_latency: 0,
                custom_metrics: None,
            },
            operation_status: Some("completed".to_string()),
            operation_id: Some(format!("op-{}", op_count)),
            pending_operations: None,
            timestamp: chrono::Utc::now().timestamp().to_string(),
        };
        
        Ok(result)
    }
    
    async fn get_regions(&self) -> Result<Vec<RegionInfo>, TeeError> {
        Ok(vec![RegionInfo {
            id: "test-region".to_string(),
            worker_ids: vec!["worker-1".to_string(), "worker-2".to_string()],
            max_tasks: 100,
        }])
    }
    
    async fn get_attestations(&self, _region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(vec![TeeAttestation {
            enclave_id: vec![1, 2, 3],
            measurement: vec![4, 5, 6],
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: vec![7, 8, 9],
            signature: vec![10, 11, 12],
            region_proof: None,
            enclave_type: TeeType::SGX,
        }])
    }
    
    async fn deploy_contract(&self, _bytecode: &[u8], _region_id: &str) -> Result<String, TeeError> {
        Ok(format!("contract-{}", rand::random::<u64>()))
    }
    
    async fn get_state_hash(&self, _contract_id: &str) -> Result<Vec<u8>, TeeError> {
        Ok(vec![0, 1, 2, 3])
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
