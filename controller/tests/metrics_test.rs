use tee_controller::metrics::{MetricsStore, RoutingStrategy, TeePerformanceMetrics};
use tokio::time::{sleep, Duration};
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::test]
async fn test_metrics_collection() {
    // Create a metrics store
    let store = MetricsStore::new();
    
    // Record a few successful executions
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-1", 
        50.0, 
        true, 
        1000, 
        500
    ).await.unwrap();
    
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-1", 
        100.0, 
        true, 
        1000, 
        500
    ).await.unwrap();
    
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-1", 
        75.0, 
        true, 
        1000, 
        500
    ).await.unwrap();
    
    // Record a failure
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-1", 
        0.0, 
        false, 
        1000, 
        0
    ).await.unwrap();
    
    // Get metrics and verify
    let metrics = store.get_worker_metrics("worker-1").await.unwrap();
    assert_eq!(metrics.total_executions, 4);
    assert_eq!(metrics.successful_executions, 3);
    assert_eq!(metrics.failed_executions, 1);
    assert!(metrics.success_rate > 0.0 && metrics.success_rate < 100.0);
}

#[tokio::test]
async fn test_routing_strategy() {
    // Create a metrics store
    let store = Arc::new(MetricsStore::new());
    
    // Record metrics for multiple workers
    store.record_execution("region-1", "SGX", "fast-worker", 50.0, true, 1000, 500).await.unwrap();
    store.record_execution("region-1", "SGX", "fast-worker", 60.0, true, 1000, 500).await.unwrap();
    
    store.record_execution("region-1", "SGX", "slow-worker", 150.0, true, 1000, 500).await.unwrap();
    store.record_execution("region-1", "SGX", "slow-worker", 180.0, true, 1000, 500).await.unwrap();
    
    store.record_execution("region-1", "SGX", "unreliable-worker", 40.0, true, 1000, 500).await.unwrap();
    store.record_execution("region-1", "SGX", "unreliable-worker", 0.0, false, 1000, 0).await.unwrap();
    store.record_execution("region-1", "SGX", "unreliable-worker", 0.0, false, 1000, 0).await.unwrap();
    
    // Create routing strategy
    let strategy = RoutingStrategy::new(Arc::clone(&store));
    
    // Test worker selection
    let candidates = vec![
        "fast-worker".to_string(), 
        "slow-worker".to_string(), 
        "unreliable-worker".to_string()
    ];
    
    let best_workers = strategy.select_best_workers(candidates.clone(), 1).await;
    println!("Selected best worker: {}", best_workers[0]);
    
    // The fast worker has 100% success rate and fastest execution time
    assert_eq!(best_workers[0], "fast-worker");
    
    // Test with a limited set of candidates
    let limited_candidates = vec![
        "slow-worker".to_string(), 
        "unreliable-worker".to_string()
    ];
    
    let best_limited = strategy.select_best_workers(limited_candidates.clone(), 1).await;
    println!("Selected best worker from limited set: {}", best_limited[0]);
    
    // The slow worker has 100% success rate, so it should be preferred over the unreliable one
    assert_eq!(best_limited[0], "slow-worker");
}

#[tokio::test]
async fn test_region_selection() {
    // Create a metrics store
    let store = Arc::new(MetricsStore::new());
    
    // Record metrics for multiple regions via worker metrics
    store.record_execution("fast-region", "SGX", "fast-worker", 50.0, true, 1000, 500).await.unwrap();
    store.record_execution("fast-region", "SGX", "fast-worker", 60.0, true, 1000, 500).await.unwrap();
    
    store.record_execution("slow-region", "SGX", "slow-worker", 150.0, true, 1000, 500).await.unwrap();
    store.record_execution("slow-region", "SGX", "slow-worker", 180.0, true, 1000, 500).await.unwrap();
    
    store.record_execution("unreliable-region", "SGX", "unreliable-worker", 40.0, true, 1000, 500).await.unwrap();
    store.record_execution("unreliable-region", "SGX", "unreliable-worker", 0.0, false, 1000, 0).await.unwrap();
    store.record_execution("unreliable-region", "SGX", "unreliable-worker", 0.0, false, 1000, 0).await.unwrap();
    
    // Create routing strategy
    let strategy = RoutingStrategy::new(Arc::clone(&store));
    
    // Test region selection - should return the current region based on proximity routing
    let current_region = "fast-region";
    let best_region = strategy.select_best_region(current_region).await.unwrap();
    
    println!("Selected best region: {}", best_region);
    
    // The routing strategy should always return the current region for proximity-based routing
    assert_eq!(best_region, current_region);
    
    // Test with a different region
    let different_region = "slow-region";
    let best_region_2 = strategy.select_best_region(different_region).await.unwrap();
    
    println!("Selected best region (2): {}", best_region_2);
    
    // Should still return the input region for proximity-based routing
    assert_eq!(best_region_2, different_region);
}

#[tokio::test]
async fn test_mesh_network_metrics() {
    // Create a metrics store
    let store = MetricsStore::new();
    
    // Record metrics for mesh network routes
    
    // Tests for SGX to SGX communication within the same region
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-sgx-1", 
        15.0, 
        true, 
        500, 
        100
    ).await.unwrap();
    
    // Tests for SGX to SEV communication within the same region
    store.record_execution(
        "region-1", 
        "SEV", 
        "worker-sev-1", 
        20.0, 
        true, 
        600, 
        120
    ).await.unwrap();
    
    // Tests for cross-region communication
    store.record_execution(
        "region-2", 
        "SGX", 
        "worker-sgx-2", 
        35.0, 
        true, 
        700, 
        150
    ).await.unwrap();
    
    // Test high latency cross-region communication
    store.record_execution(
        "region-3", 
        "SGX", 
        "worker-sgx-3", 
        120.0, 
        true, 
        900, 
        200
    ).await.unwrap();
    
    // Verify metrics for different workers
    let sgx_1_metrics = store.get_worker_metrics("worker-sgx-1").await.unwrap();
    let sev_1_metrics = store.get_worker_metrics("worker-sev-1").await.unwrap();
    let sgx_2_metrics = store.get_worker_metrics("worker-sgx-2").await.unwrap();
    let sgx_3_metrics = store.get_worker_metrics("worker-sgx-3").await.unwrap();
    
    // Verify execution time metrics
    assert!(sgx_1_metrics.last_execution_time_ms < 20); 
    assert!(sev_1_metrics.last_execution_time_ms < 25); 
    assert!(sgx_2_metrics.last_execution_time_ms < 40); 
    assert!(sgx_3_metrics.last_execution_time_ms > 100); 
    
    // Create routing strategy
    let store_arc = Arc::new(store);
    let strategy = RoutingStrategy::new(Arc::clone(&store_arc));
    
    // Test worker selection based on mesh network performance
    let candidates = vec![
        "worker-sgx-1".to_string(),
        "worker-sev-1".to_string(),
        "worker-sgx-2".to_string(),
        "worker-sgx-3".to_string(),
    ];
    
    let best_workers = strategy.select_best_workers(candidates.clone(), 2).await;
    println!("Selected best workers: {:?}", best_workers);
    
    // We should get the two workers with lowest latency (worker-sgx-1 and worker-sev-1)
    assert!(best_workers.contains(&"worker-sgx-1".to_string()));
    assert!(best_workers.contains(&"worker-sev-1".to_string()));
}

#[tokio::test]
async fn test_dual_execution_path_selection() {
    // Create a metrics store
    let store = Arc::new(MetricsStore::new());
    
    // Record metrics for direct worker and coordinator paths
    
    // Direct worker execution (mesh path)
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-mesh-1", 
        15.0, 
        true, 
        500, 
        100
    ).await.unwrap();
    
    // Another direct worker with slightly higher latency
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-mesh-2", 
        25.0, 
        true, 
        550, 
        110
    ).await.unwrap();
    
    // Coordinator mediated execution
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-coordinator-1", 
        120.0, 
        true, 
        700, 
        150
    ).await.unwrap();
    
    // Another coordinator worker
    store.record_execution(
        "region-1", 
        "SGX", 
        "worker-coordinator-2", 
        150.0, 
        true, 
        750, 
        160
    ).await.unwrap();
    
    // Create routing strategy
    let strategy = RoutingStrategy::new(Arc::clone(&store));
    
    // Test worker selection
    let all_candidates = vec![
        "worker-mesh-1".to_string(),
        "worker-mesh-2".to_string(),
        "worker-coordinator-1".to_string(),
        "worker-coordinator-2".to_string(),
    ];
    
    let best_workers = strategy.select_best_workers(all_candidates.clone(), 2).await;
    println!("Selected best workers: {:?}", best_workers);
    
    // Direct workers should be preferred due to lower latency
    assert!(best_workers.contains(&"worker-mesh-1".to_string()));
    assert!(best_workers.contains(&"worker-mesh-2".to_string()));
    
    // If we only have coordinator workers
    let coordinator_only = vec![
        "worker-coordinator-1".to_string(),
        "worker-coordinator-2".to_string(),
    ];
    
    let best_coordinator = strategy.select_best_workers(coordinator_only.clone(), 1).await;
    println!("Selected best coordinator worker: {}", best_coordinator[0]);
    
    // Should select the coordinator worker with best performance
    assert_eq!(best_coordinator[0], "worker-coordinator-1");
    
    // Test fallback from direct to coordinator
    let mixed_candidates = vec![
        "worker-mesh-1".to_string(), 
        "worker-coordinator-1".to_string(), 
    ];
    
    let best_mixed = strategy.select_best_workers(mixed_candidates.clone(), 2).await;
    println!("Selected from mixed workers: {:?}", best_mixed);
    
    // Should prioritize direct mesh, then fallback to coordinator
    assert_eq!(best_mixed[0], "worker-mesh-1");
    assert_eq!(best_mixed[1], "worker-coordinator-1");
}
