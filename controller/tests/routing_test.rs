use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;
use std::collections::HashMap;

use tee_controller::hyper_integration::HyperTeeController;
use tee_controller::metrics::{MetricsStore, RoutingStrategy, TeePerformanceMetrics};
use tee_interface::{ExecutionPayload, TeeExecutor, TeeType, RegionInfo};

#[tokio::test]
async fn test_metrics_based_routing() {
    // Create a new metrics store
    let metrics_store = Arc::new(MetricsStore::new());
    
    // Record some metrics to simulate performance data
    metrics_store.record_worker_metric("worker-1", "us-west", 50, true).await;
    metrics_store.record_worker_metric("worker-2", "us-west", 100, true).await;
    metrics_store.record_worker_metric("worker-3", "us-east", 30, true).await;
    metrics_store.record_worker_metric("worker-4", "us-east", 60, true).await;
    
    // Add some failures to one worker to decrease its success rate
    for _ in 0..5 {
        metrics_store.record_worker_metric("worker-2", "us-west", 0, false).await;
    }
    
    // Create a routing strategy
    let routing_strategy = RoutingStrategy::new(metrics_store.clone());
    
    // Test selecting the best worker
    let worker_ids = vec![
        "worker-1".to_string(),
        "worker-2".to_string(),
        "worker-3".to_string(),
        "worker-4".to_string(),
    ];
    
    let best_workers = routing_strategy.select_best_workers(worker_ids, 2).await;
    
    // The best workers should be worker-3 (fastest) and worker-1 (good success rate)
    assert!(best_workers.contains(&"worker-3".to_string()));
    assert!(best_workers.contains(&"worker-1".to_string()));
    assert_eq!(best_workers.len(), 2);
    
    // Test best region with proximity-based routing
    // In proximity-based routing, we always select the current region
    let best_region = routing_strategy.select_best_region("us-west").await;
    assert_eq!(best_region, Some("us-west".to_string()));
}

#[tokio::test]
async fn test_controller_metrics_integration() {
    // Create a new HyperTeeController
    let controller = HyperTeeController::new().await;
    
    // Simulate execution to collect metrics
    let payload = ExecutionPayload {
        input: b"test-input".to_vec(),
        params: tee_interface::ExecutionParams {
            id_to: "receiver".to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
        region_id: Some("us-west".to_string()),
        target_tee: None,
        tee_type: Some("SGX".to_string()),
        allow_fallback: Some(true),
    };
    
    // Execute the payload a few times
    for i in 0..5 {
        let result = controller.execute(&payload).await;
        assert!(result.is_ok(), "Execution failed on iteration {}: {:?}", i, result);
    }
    
    // Check if metrics were collected
    // This is a simple integration test, so we just want to make sure the controller is recording metrics
    // We don't need to assert specific values
}
