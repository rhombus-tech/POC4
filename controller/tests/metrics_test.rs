use tee_controller::metrics::{MetricsStore, RoutingStrategy, TeePerformanceMetrics};
use tokio::time::{sleep, Duration};
use std::sync::Arc;

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
