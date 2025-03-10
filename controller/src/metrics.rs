use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

/// Performance metrics for TEE instances
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeePerformanceMetrics {
    /// Total number of executions processed
    pub total_executions: u64,
    
    /// Number of successful executions
    pub successful_executions: u64,
    
    /// Number of failed executions
    pub failed_executions: u64,
    
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    
    /// Most recent execution time in milliseconds
    pub last_execution_time_ms: u64,
    
    /// Success rate as a percentage (0-100)
    pub success_rate: f64,
    
    /// Time of the last successful execution
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub last_success_time: Option<DateTime<Utc>>,
    
    /// Time of the last failed execution
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub last_failure_time: Option<DateTime<Utc>>,
}

impl TeePerformanceMetrics {
    /// Create a new empty metrics instance
    pub fn new() -> Self {
        Self {
            min_execution_time_ms: u64::MAX, // Start with max value so first execution will be the min
            ..Default::default()
        }
    }
    
    /// Record a successful execution with the given duration
    pub fn record_success(&mut self, duration_ms: u64) {
        self.total_executions += 1;
        self.successful_executions += 1;
        self.last_execution_time_ms = duration_ms;
        self.last_success_time = Some(Utc::now());
        
        // Update min/max times
        self.min_execution_time_ms = self.min_execution_time_ms.min(duration_ms);
        self.max_execution_time_ms = self.max_execution_time_ms.max(duration_ms);
        
        // Recalculate average
        if self.total_executions == 1 {
            self.average_execution_time_ms = duration_ms;
        } else {
            // Calculate running average
            let total_time = self.average_execution_time_ms * (self.total_executions - 1);
            self.average_execution_time_ms = (total_time + duration_ms) / self.total_executions;
        }
        
        // Update success rate
        self.success_rate = (self.successful_executions as f64 / self.total_executions as f64) * 100.0;
    }
    
    /// Record a failed execution
    pub fn record_failure(&mut self) {
        self.total_executions += 1;
        self.failed_executions += 1;
        self.last_failure_time = Some(Utc::now());
        
        // Update success rate
        self.success_rate = (self.successful_executions as f64 / self.total_executions as f64) * 100.0;
    }
}

/// Storage for TEE metrics and execution statistics
#[derive(Debug, Clone)]
pub struct MetricsStore {
    /// Metrics by worker ID (TEE instance)
    worker_metrics: Arc<RwLock<HashMap<String, TeePerformanceMetrics>>>,
    
    /// Metrics by region ID
    region_metrics: Arc<RwLock<HashMap<String, TeePerformanceMetrics>>>,
    
    /// Overall metrics for the controller
    overall_metrics: Arc<RwLock<TeePerformanceMetrics>>,
    
    /// TEE type metrics
    tee_type_metrics: Arc<RwLock<HashMap<String, TeePerformanceMetrics>>>,
    
    /// Current worker ID for this instance
    worker_id: String,
    
    /// Current region ID for this instance
    region_id: String,
    
    /// TEE type for this instance
    tee_type: String,
}

impl MetricsStore {
    /// Create a new metrics store
    pub fn new(worker_id: String, region_id: String, tee_type: String) -> Self {
        let mut worker_metrics = HashMap::new();
        worker_metrics.insert(worker_id.clone(), TeePerformanceMetrics::new());
        
        let mut region_metrics = HashMap::new();
        region_metrics.insert(region_id.clone(), TeePerformanceMetrics::new());
        
        let mut tee_type_metrics = HashMap::new();
        tee_type_metrics.insert(tee_type.clone(), TeePerformanceMetrics::new());
        
        Self {
            worker_metrics: Arc::new(RwLock::new(worker_metrics)),
            region_metrics: Arc::new(RwLock::new(region_metrics)),
            overall_metrics: Arc::new(RwLock::new(TeePerformanceMetrics::new())),
            tee_type_metrics: Arc::new(RwLock::new(tee_type_metrics)),
            worker_id,
            region_id,
            tee_type,
        }
    }
    
    /// Record a successful execution for this worker
    pub async fn record_execution_success(&self, duration_ms: u64) -> Result<(), String> {
        // Update worker metrics
        {
            let mut metrics = self.worker_metrics.write().await;
            let worker_metrics = metrics.entry(self.worker_id.clone())
                .or_insert_with(TeePerformanceMetrics::new);
            worker_metrics.record_success(duration_ms);
        }
        
        // Update region metrics
        {
            let mut metrics = self.region_metrics.write().await;
            let region_metrics = metrics.entry(self.region_id.clone())
                .or_insert_with(TeePerformanceMetrics::new);
            region_metrics.record_success(duration_ms);
        }
        
        // Update TEE type metrics
        {
            let mut metrics = self.tee_type_metrics.write().await;
            let tee_type_metrics = metrics.entry(self.tee_type.clone())
                .or_insert_with(TeePerformanceMetrics::new);
            tee_type_metrics.record_success(duration_ms);
        }
        
        // Update overall metrics
        {
            let mut metrics = self.overall_metrics.write().await;
            metrics.record_success(duration_ms);
        }
        
        Ok(())
    }
    
    /// Record a failed execution for this worker
    pub async fn record_execution_failure(&self) -> Result<(), String> {
        // Update worker metrics
        {
            let mut metrics = self.worker_metrics.write().await;
            let worker_metrics = metrics.entry(self.worker_id.clone())
                .or_insert_with(TeePerformanceMetrics::new);
            worker_metrics.record_failure();
        }
        
        // Update region metrics
        {
            let mut metrics = self.region_metrics.write().await;
            let region_metrics = metrics.entry(self.region_id.clone())
                .or_insert_with(TeePerformanceMetrics::new);
            region_metrics.record_failure();
        }
        
        // Update TEE type metrics
        {
            let mut metrics = self.tee_type_metrics.write().await;
            let tee_type_metrics = metrics.entry(self.tee_type.clone())
                .or_insert_with(TeePerformanceMetrics::new);
            tee_type_metrics.record_failure();
        }
        
        // Update overall metrics
        {
            let mut metrics = self.overall_metrics.write().await;
            metrics.record_failure();
        }
        
        Ok(())
    }
    
    /// Get metrics for a specific worker
    pub async fn get_worker_metrics(&self, worker_id: &str) -> Option<TeePerformanceMetrics> {
        let metrics = self.worker_metrics.read().await;
        metrics.get(worker_id).cloned()
    }
    
    /// Get metrics for a specific region
    pub async fn get_region_metrics(&self, region_id: &str) -> Option<TeePerformanceMetrics> {
        let metrics = self.region_metrics.read().await;
        metrics.get(region_id).cloned()
    }
    
    /// Get metrics for a specific TEE type
    pub async fn get_tee_type_metrics(&self, tee_type: &str) -> Option<TeePerformanceMetrics> {
        let metrics = self.tee_type_metrics.read().await;
        metrics.get(tee_type).cloned()
    }
    
    /// Get overall metrics for the controller
    pub async fn get_overall_metrics(&self) -> TeePerformanceMetrics {
        let metrics = self.overall_metrics.read().await;
        metrics.clone()
    }
    
    /// Get metrics for this worker
    pub async fn get_current_worker_metrics(&self) -> TeePerformanceMetrics {
        self.get_worker_metrics(&self.worker_id).await
            .unwrap_or_else(TeePerformanceMetrics::new)
    }

    /// Record metrics for a specific worker
    pub async fn record_worker_success(&self, worker_id: &str, duration_ms: u64) -> Result<(), String> {
        let mut metrics = self.worker_metrics.write().await;
        let worker_metrics = metrics.entry(worker_id.to_string())
            .or_insert_with(TeePerformanceMetrics::new);
        worker_metrics.record_success(duration_ms);
        Ok(())
    }
    
    /// Record failure for a specific worker
    pub async fn record_worker_failure(&self, worker_id: &str) -> Result<(), String> {
        let mut metrics = self.worker_metrics.write().await;
        let worker_metrics = metrics.entry(worker_id.to_string())
            .or_insert_with(TeePerformanceMetrics::new);
        worker_metrics.record_failure();
        Ok(())
    }
    
    /// Helper method to record metrics for a specific worker in a specific region
    /// Used primarily for testing
    pub async fn record_worker_metric(&self, worker_id: &str, region_id: &str, duration_ms: u64, success: bool) -> Result<(), String> {
        // Update worker metrics
        if success {
            self.record_worker_success(worker_id, duration_ms).await?;
        } else {
            self.record_worker_failure(worker_id).await?;
        }
        
        // Update region metrics
        {
            let mut region_metrics = self.region_metrics.write().await;
            let metrics = region_metrics.entry(region_id.to_string())
                .or_insert_with(TeePerformanceMetrics::new);
                
            if success {
                metrics.record_success(duration_ms);
            } else {
                metrics.record_failure();
            }
        }
        
        Ok(())
    }
    
    /// Get all region metrics
    pub async fn get_all_region_metrics(&self) -> HashMap<String, TeePerformanceMetrics> {
        self.region_metrics.read().await.clone()
    }
}

/// A structure that guides TEE selection based on performance metrics
pub struct RoutingStrategy {
    /// The metrics store to use for decisions
    metrics: Arc<MetricsStore>,
}

impl RoutingStrategy {
    /// Create a new routing strategy with the given metrics store
    pub fn new(metrics: Arc<MetricsStore>) -> Self {
        Self { metrics }
    }
    
    /// Select the best workers from a list of candidates based on performance metrics
    pub async fn select_best_workers(&self, worker_ids: Vec<String>, count: usize) -> Vec<String> {
        if worker_ids.is_empty() || count == 0 {
            return Vec::new();
        }
        
        // Get metrics for all candidates
        let mut worker_metrics = Vec::new();
        for worker_id in &worker_ids {
            if let Some(metrics) = self.metrics.get_worker_metrics(worker_id).await {
                worker_metrics.push((worker_id.clone(), metrics));
            }
        }
        
        // If we don't have metrics for any candidate, return the first few
        if worker_metrics.is_empty() {
            return worker_ids.into_iter().take(count).collect();
        }
        
        // Sort by success rate (highest first) and then by average execution time (lowest first)
        worker_metrics.sort_by(|a, b| {
            // First compare by success rate (descending)
            let success_cmp = b.1.success_rate.partial_cmp(&a.1.success_rate).unwrap_or(std::cmp::Ordering::Equal);
            
            // If success rates are close (within 5%), compare by average execution time
            if (b.1.success_rate - a.1.success_rate).abs() < 5.0 {
                a.1.average_execution_time_ms.partial_cmp(&b.1.average_execution_time_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                success_cmp
            }
        });
        
        // Return the best candidates up to count
        worker_metrics.into_iter()
            .take(count)
            .map(|(id, _)| id)
            .collect()
    }
    
    /// Select the best region based on performance metrics
    pub async fn select_best_region(&self, current_region: &str) -> Option<String> {
        // This method was originally designed to select the best region based on performance metrics.
        // However, according to our regional mesh network architecture, routing should be proximity-based,
        // not performance-based. This method is kept for backwards compatibility but will simply return
        // the current region.
        Some(current_region.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_metrics_recording() {
        let metrics = TeePerformanceMetrics::new();
        assert_eq!(metrics.total_executions, 0);
        assert_eq!(metrics.successful_executions, 0);
        assert_eq!(metrics.failed_executions, 0);
        
        let mut metrics = TeePerformanceMetrics::new();
        metrics.record_success(100);
        assert_eq!(metrics.total_executions, 1);
        assert_eq!(metrics.successful_executions, 1);
        assert_eq!(metrics.failed_executions, 0);
        assert_eq!(metrics.average_execution_time_ms, 100);
        assert_eq!(metrics.min_execution_time_ms, 100);
        assert_eq!(metrics.max_execution_time_ms, 100);
        assert_eq!(metrics.success_rate, 100.0);
        
        metrics.record_success(200);
        assert_eq!(metrics.total_executions, 2);
        assert_eq!(metrics.successful_executions, 2);
        assert_eq!(metrics.failed_executions, 0);
        assert_eq!(metrics.average_execution_time_ms, 150); // (100 + 200) / 2
        assert_eq!(metrics.min_execution_time_ms, 100);
        assert_eq!(metrics.max_execution_time_ms, 200);
        assert_eq!(metrics.success_rate, 100.0);
        
        metrics.record_failure();
        assert_eq!(metrics.total_executions, 3);
        assert_eq!(metrics.successful_executions, 2);
        assert_eq!(metrics.failed_executions, 1);
        assert_eq!(metrics.average_execution_time_ms, 150); // Failures don't affect timing
        assert_eq!(metrics.success_rate, 66.66666666666666);
    }
    
    #[tokio::test]
    async fn test_metrics_store() {
        let store = MetricsStore::new(
            "worker-1".to_string(),
            "region-1".to_string(),
            "SGX".to_string()
        );
        
        // Record successful execution
        store.record_execution_success(150).await.unwrap();
        
        // Get metrics for the worker
        let worker_metrics = store.get_worker_metrics("worker-1").await.unwrap();
        assert_eq!(worker_metrics.total_executions, 1);
        assert_eq!(worker_metrics.successful_executions, 1);
        assert_eq!(worker_metrics.average_execution_time_ms, 150);
        
        // Get metrics for the region
        let region_metrics = store.get_region_metrics("region-1").await.unwrap();
        assert_eq!(region_metrics.total_executions, 1);
        assert_eq!(region_metrics.successful_executions, 1);
        
        // Record failure
        store.record_execution_failure().await.unwrap();
        
        // Get updated metrics
        let worker_metrics = store.get_worker_metrics("worker-1").await.unwrap();
        assert_eq!(worker_metrics.total_executions, 2);
        assert_eq!(worker_metrics.successful_executions, 1);
        assert_eq!(worker_metrics.failed_executions, 1);
        assert_eq!(worker_metrics.success_rate, 50.0);
    }
    
    #[tokio::test]
    async fn test_routing_strategy() {
        let store = Arc::new(MetricsStore::new(
            "worker-1".to_string(),
            "region-1".to_string(),
            "SGX".to_string()
        ));
        
        // Record metrics for several workers
        store.record_worker_success("worker-1", 100).await.unwrap();
        store.record_worker_success("worker-1", 150).await.unwrap();
        store.record_worker_success("worker-2", 50).await.unwrap();
        store.record_worker_success("worker-2", 80).await.unwrap();
        store.record_worker_success("worker-3", 200).await.unwrap();
        store.record_worker_failure("worker-3").await.unwrap();
        
        let strategy = RoutingStrategy::new(store);
        
        // Test worker selection
        let candidates = vec!["worker-1".to_string(), "worker-2".to_string(), "worker-3".to_string()];
        let best_workers = strategy.select_best_workers(candidates, 2).await;
        
        // worker-2 has 100% success rate and lowest average time (65ms)
        assert_eq!(best_workers.len(), 2);
    }
}
