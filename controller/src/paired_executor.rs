use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, error, debug, warn};
use tee_interface::{TeeError, TeeExecutor, ExecutionPayload, ExecutionResult, TeeAttestation, RegionInfo};
use async_trait::async_trait;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use chrono::Utc;
use crate::mesh::{MeshCoordinator, MeshExecutionResult, PeerInfo, SyncResult, TeeType as MeshTeeType};
use std::collections::{HashMap, VecDeque};

/// TeeExecutorPair combines two TeeExecutor instances
/// for redundant execution and cross-checking results
pub struct TeeExecutorPair {
    /// Primary TEE executor
    primary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
    /// Secondary TEE executor
    secondary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
    /// Optional contract ID generator function for consistent IDs across TEEs
    contract_id_generator: Option<fn(&str) -> String>,
    /// Optional mesh coordinator for direct TEE-to-TEE communication
    mesh_coordinator: Option<Arc<MeshCoordinator>>,
    /// Performance metrics store for routing decisions
    metrics_store: RwLock<HashMap<String, MetricsData>>,
    /// Circuit breaker for mesh execution
    mesh_circuit_breaker: RwLock<CircuitBreakerState>,
}

/// Data structure to track performance metrics for various execution paths
struct MetricsData {
    /// Rolling window of execution times in milliseconds
    execution_times: VecDeque<u64>,
    /// Count of successful executions
    success_count: u64,
    /// Count of failed executions
    failure_count: u64,
    /// Last execution timestamp
    last_execution: chrono::DateTime<Utc>,
}

/// Circuit breaker pattern for mesh execution
struct CircuitBreakerState {
    /// Whether the circuit breaker is currently tripped
    is_tripped: bool,
    /// When the circuit breaker was tripped
    tripped_at: Option<chrono::DateTime<Utc>>,
    /// Consecutive failure count that led to tripping
    failure_count: u64,
    /// Reset timeout in seconds
    reset_timeout_sec: u64,
    /// Consecutive failure threshold to trip
    failure_threshold: u64,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            is_tripped: false,
            tripped_at: None,
            failure_count: 0,
            reset_timeout_sec: 300, // 5 minutes default
            failure_threshold: 5,   // 5 consecutive failures
        }
    }
}

impl Default for MetricsData {
    fn default() -> Self {
        Self {
            execution_times: VecDeque::with_capacity(100), // Keep last 100 execution times
            success_count: 0,
            failure_count: 0,
            last_execution: Utc::now(),
        }
    }
}

impl TeeExecutorPair {
    /// Create a new TeeExecutorPair with primary and secondary executors
    pub fn new(
        primary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
        secondary: Arc<RwLock<dyn TeeExecutor + Send + Sync>>,
        mesh_coordinator: Option<Arc<MeshCoordinator>>,
    ) -> Self {
        Self { 
            primary, 
            secondary,
            contract_id_generator: None,
            mesh_coordinator,
            metrics_store: RwLock::new(HashMap::new()),
            mesh_circuit_breaker: RwLock::new(CircuitBreakerState::default()),
        }
    }
    
    /// Set a custom contract ID generator function
    pub fn with_contract_id_generator(mut self, generator: fn(&str) -> String) -> Self {
        self.contract_id_generator = Some(generator);
        self
    }
    
    /// Execute a task via the mesh network
    pub async fn execute_mesh(
        &self,
        target_tee: String,
        region: String,
        tee_type: MeshTeeType,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        info!("Executing task via mesh network: target={}, region={}", target_tee, region);
        
        match &self.mesh_coordinator {
            Some(coordinator) => {
                coordinator.execute(
                    target_tee, 
                    region, 
                    tee_type.to_string(),
                    input,
                    timeout,
                    is_async,
                    allow_fallback
                ).await
            },
            None => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Mesh coordinator not initialized"
                ))
            }
        }
    }
    
    /// Discover peers in the mesh network
    pub async fn discover_peers(
        &self,
        region: String,
        tee_type: Option<MeshTeeType>,
        max_results: usize,
    ) -> Result<Vec<PeerInfo>, std::io::Error> {
        info!("Discovering peers in region: {}", region);
        
        match &self.mesh_coordinator {
            Some(coordinator) => {
                coordinator.discover_peers(region, tee_type.map(|t| t.to_string()), max_results).await
            },
            None => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Mesh coordinator not initialized"
                ))
            }
        }
    }
    
    /// Synchronize state with another TEE
    pub async fn sync_state(
        &self,
        object_id: String,
        target_tee: String,
        use_deltas: bool,
    ) -> Result<SyncResult, std::io::Error> {
        info!("Synchronizing state with TEE: {}", target_tee);
        
        match &self.mesh_coordinator {
            Some(coordinator) => {
                coordinator.sync_state(object_id, target_tee, use_deltas).await
            },
            None => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Mesh coordinator not initialized"
                ))
            }
        }
    }
    
    /// Execute a task via the mesh network with caching
    pub async fn execute_with_mesh_cache(
        &self,
        target_tee: String,
        region: String,
        tee_type: MeshTeeType,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
        use_cache: bool,
        cache_ttl: Duration,
        stale_result_timeout: Duration,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        // If mesh coordinator is available, try to execute via mesh with caching
        if let Some(mesh) = &self.mesh_coordinator {
            info!("Executing via mesh with caching in region: {}, target: {}", region, target_tee);
            
            // Use execute_with_cache instead of execute_with_mesh_cache
            mesh.execute_with_cache(
                target_tee,
                region,
                tee_type.to_string(),
                input,
                timeout,
                is_async,
                allow_fallback,
                use_cache,
                cache_ttl,
                stale_result_timeout,
            ).await
        } else {
            error!("Mesh coordinator is not available for execution");
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Mesh coordinator is not available"
            ))
        }
    }
    
    /// Verify if both TEE platforms are available
    pub async fn verify_platforms(&self) -> (bool, bool) {
        debug!("Verifying platform availability");
        
        let sgx_available = match self.primary.read().await.get_attestations("default").await {
            Ok(_) => true,
            Err(_) => false,
        };
        
        let sev_available = match self.secondary.read().await.get_attestations("default").await {
            Ok(_) => true,
            Err(_) => false,
        };
        
        (sgx_available, sev_available)
    }
    
    /// Execute a task on both TEEs in paired mode
    pub async fn execute_paired(
        &self,
        wasm_module: PathBuf,
        input: Vec<u8>,
        contract_id: String,
        operation_id: String,
        _timeout: Duration,
        _use_cache: bool,
        function_call: Option<String>,
    ) -> Result<ExecutionResult, std::io::Error> {
        info!("Executing in paired mode with operation_id: {}", operation_id);
        
        // Read WASM module - keep this in case we need the code later
        let _wasm_code = match tokio::fs::read(&wasm_module).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read WASM module: {:?}", e);
                return Err(e);
            }
        };
        
        // Create execution payload with proper structure
        let mut payload = ExecutionPayload::default();
        
        // Set payload fields
        payload.operation_id = Some(operation_id.clone());
        payload.input = input;
        
        // Update the params
        payload.params.id_to = contract_id;
        payload.params.function_call = function_call.unwrap_or_else(|| "main".to_string());
        payload.params.detailed_proof = true;
        
        // Execute on both TEEs
        match self.execute(&payload).await {
            Ok(result) => Ok(result),
            Err(e) => {
                error!("Paired execution failed: {:?}", e);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Paired execution error: {}", e)
                ))
            }
        }
    }
}

#[async_trait]
impl TeeExecutor for TeeExecutorPair {
    async fn deploy_contract(&self, wasm_code: &[u8], region_id: &str) -> Result<String, TeeError> {
        // If we have a custom contract ID generator, use it to generate a consistent ID
        if let Some(generator) = self.contract_id_generator {
            let contract_id = generator(region_id);
            info!("Using generated contract ID {} for region {}", contract_id, region_id);
            
            // Deploy to primary TEE with custom ID
            let primary_executor = self.primary.read().await;
            (*primary_executor).deploy_contract(wasm_code, region_id).await?;
            
            // Deploy to secondary TEE with custom ID
            let secondary_executor = self.secondary.read().await;
            (*secondary_executor).deploy_contract(wasm_code, region_id).await?;
            
            return Ok(contract_id);
        }
        
        // Standard deployment flow - deploy to both TEEs and ensure they match
        info!("Deploying contract to both TEEs in region {}", region_id);
        
        // Deploy to primary TEE
        let primary_executor = self.primary.read().await;
        let primary_id = (*primary_executor).deploy_contract(wasm_code, region_id).await?;
        
        // Deploy to secondary TEE
        let secondary_executor = self.secondary.read().await;
        let secondary_id = (*secondary_executor).deploy_contract(wasm_code, region_id).await?;
        
        // Verify both TEEs generated the same contract ID
        if primary_id != secondary_id {
            warn!("Contract IDs from primary and secondary TEEs don't match: {} vs {}", 
                 primary_id, secondary_id);
            return Err(TeeError::Contract("Contract IDs from primary and secondary TEEs don't match".to_string()));
        }
        
        info!("Contract deployed successfully on both TEEs with ID {}", primary_id);
        Ok(primary_id)
    }
    
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let default_op_id = "unknown".to_string();
        let operation_id = payload.operation_id.as_ref().unwrap_or(&default_op_id);
        info!("Executing contract for operation ID {}", operation_id);
        
        // Extract region ID from payload
        let region_id = match &payload.region_id {
            Some(region) => region.clone(),
            None => {
                warn!("No region ID specified in payload, defaulting to local execution");
                return execute_coordinator(self, payload).await;
            }
        };
        
        // Track start time for overall execution
        let start_time = Utc::now();
        
        // Check circuit breaker status
        if let true = is_circuit_breaker_tripped(self).await {
            info!("Mesh circuit breaker is tripped, using coordinator execution for operation {}", operation_id);
            return execute_coordinator(self, payload).await;
        }
        
        // Check if mesh execution is appropriate
        if let Some(ref mesh_coordinator) = self.mesh_coordinator {
            // Check if target_tee and tee_type are specified
            if let (Some(target_tee), Some(tee_type_str)) = (&payload.target_tee, &payload.tee_type) {
                // Check if we should use mesh based on metrics
                let mesh_key = format!("mesh:{}:{}:{}", region_id, target_tee, tee_type_str);
                let coordinator_key = format!("coordinator:{}:{}", region_id, operation_id);
                
                let should_use_mesh = should_use_mesh_execution(self, &mesh_key, &coordinator_key).await;
                
                if !should_use_mesh {
                    debug!("Based on performance metrics, using coordinator execution for operation {}", operation_id);
                    return execute_coordinator(self, payload).await;
                }
                
                // Attempt mesh execution
                info!("Attempting mesh execution for operation {} to target {} in region {}", 
                      operation_id, target_tee, region_id);
                
                // Serialize the payload for mesh execution
                let input = match bincode::serialize(payload) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Failed to serialize payload for mesh execution: {}", e);
                        let _ = update_metrics(self, &mesh_key, None, false).await;
                        return execute_coordinator(self, payload).await;
                    }
                };
                
                // Default timeout of 5 seconds, can be adjusted based on payload requirements
                let timeout = Duration::from_secs(5);
                
                // Execute via mesh with fallback allowed
                match self.execute_mesh(
                    target_tee.clone(), 
                    region_id.clone(), 
                    tee_type_str.parse().unwrap_or(MeshTeeType::IntelSGX), 
                    input,
                    timeout,
                    false, // Not async for now
                    true   // Allow fallback to coordinator if mesh fails
                ).await {
                    Ok(mesh_result) => {
                        // Mesh execution succeeded, convert result to ExecutionResult
                        let execution_time_ms = mesh_result.network_latency_ns / 1_000_000;
                        info!("Mesh execution succeeded in {}ms", execution_time_ms);
                        
                        // Update metrics with successful execution
                        let _ = update_metrics(self, &mesh_key, Some(execution_time_ms as u64), true).await;
                        
                        // Create result with data from mesh execution
                        let attestations = mesh_result.attestations.unwrap_or_default().iter()
                            .map(|a| TeeAttestation {
                                enclave_type: match a.enclave_type.as_str() {
                                    "IntelSGX" => tee_interface::TeeType::SGX,
                                    "SEV" => tee_interface::TeeType::SEV,
                                    _ => tee_interface::TeeType::SGX,
                                },
                                enclave_id: a.measurement.clone(), // Using measurement as enclave_id
                                measurement: a.measurement.clone(),
                                timestamp: a.timestamp,
                                data: a.platform_data.clone(),
                                signature: vec![],
                                region_proof: None,
                            })
                            .collect();
                        
                        // Calculate total execution time
                        let execution_time = Utc::now().signed_duration_since(start_time).num_milliseconds() as u64;
                        
                        // Update metrics with successful mesh execution data
                        let mut stats = tee_interface::ExecutionStats {
                            execution_time,
                            ..Default::default()
                        };
                        
                        // If mesh_result has metrics, populate stats with them
                        if let Some(ref metrics) = mesh_result.metrics {
                            stats.memory_used = metrics.memory_used_bytes;
                            stats.syscall_count = metrics.syscall_count;
                            stats.network_latency = metrics.network_latency_ms as u64;
                            
                            // Add custom metrics for mesh execution
                            let mut custom_metrics = HashMap::new();
                            custom_metrics.insert("execution_type".to_string(), "mesh".to_string());
                            custom_metrics.insert("target_tee".to_string(), target_tee.clone());
                            custom_metrics.insert("region_id".to_string(), region_id.clone());
                            custom_metrics.insert("tee_type".to_string(), tee_type_str.clone());
                            
                            if let Some(ops_per_second) = metrics.operations_per_second {
                                custom_metrics.insert("operations_per_second".to_string(), ops_per_second.to_string());
                            }
                            
                            stats.custom_metrics = Some(custom_metrics);
                        }
                        
                        // Create and return the final execution result
                        let result = ExecutionResult {
                            result: mesh_result.result,
                            attestations,
                            stats,
                            operation_id: payload.operation_id.clone(),
                            operation_status: Some("completed".to_string()),
                            pending_operations: Some(vec![]),
                            timestamp: SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs().to_string(),
                            state_hash: vec![], // Add empty state_hash
                        };
                        
                        info!("Contract executed successfully via mesh network");
                        return Ok(result);
                    },
                    Err(e) => {
                        // Mesh execution failed, log and fall back to coordinator
                        warn!("Mesh execution failed, falling back to coordinator: {}", e);
                        
                        // Update metrics with failed execution
                        let _ = update_metrics(self, &mesh_key, None, false).await;
                        
                        // Increment circuit breaker failure count
                        let _ = increment_circuit_breaker_failures(self).await;
                        
                        // Continue with coordinator execution below
                    }
                }
            }
        }
        
        // If we reach here, either mesh execution was not appropriate or it failed
        // Fall back to coordinator execution
        info!("Using coordinator execution for operation {}", operation_id);
        
        // Track coordinator execution start time
        let coordinator_start = Utc::now();
        
        // Call the primary executor to perform the execution
        let primary = self.primary.read().await;
        let result = primary.execute(payload).await;
        
        // Update coordinator metrics if successful
        if result.is_ok() {
            let coordinator_key = format!("coordinator:{}:{}", 
                                         payload.region_id.as_ref().unwrap_or(&"unknown".to_string()),
                                         operation_id);
            let execution_time = coordinator_start.signed_duration_since(Utc::now()).num_milliseconds().abs() as u64;
            let _ = update_metrics(self, &coordinator_key, Some(execution_time), true).await;
        }
        
        result
    }
    
    async fn get_regions(&self) -> Result<Vec<RegionInfo>, TeeError> {
        // Get regions from both TEEs
        let primary_executor = self.primary.read().await;
        let primary_regions = (*primary_executor).get_regions().await?;
        
        let secondary_executor = self.secondary.read().await;
        let secondary_regions = (*secondary_executor).get_regions().await?;
        
        // Combine regions from both TEEs
        let mut all_regions = primary_regions;
        all_regions.extend(secondary_regions);
        
        Ok(all_regions)
    }
    
    async fn get_attestations(&self, region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        // Get attestations from both TEEs
        let primary_executor = self.primary.read().await;
        let primary_attestations = (*primary_executor).get_attestations(region_id).await?;
        
        let secondary_executor = self.secondary.read().await;
        let secondary_attestations = (*secondary_executor).get_attestations(region_id).await?;
        
        // Combine attestations from both TEEs
        let mut all_attestations = primary_attestations;
        all_attestations.extend(secondary_attestations);
        
        Ok(all_attestations)
    }
    
    async fn get_state_hash(&self, contract_address: &str) -> Result<Vec<u8>, TeeError> {
        // Get state hash from primary TEE
        let primary_executor = self.primary.read().await;
        let primary_hash = (*primary_executor).get_state_hash(contract_address).await?;
        
        // Get state hash from secondary TEE
        let secondary_executor = self.secondary.read().await;
        let secondary_hash = (*secondary_executor).get_state_hash(contract_address).await?;
        
        // Verify both hashes match
        if primary_hash != secondary_hash {
            error!("State hash mismatch between primary and secondary TEEs!");
            return Err(TeeError::Contract("State hash mismatch between primary and secondary TEEs".to_string()));
        }
        
        // Return the hash (they're identical)
        Ok(primary_hash)
    }
}

// Helper methods for metrics and circuit breaker management

/// Update performance metrics for a specific execution path
async fn update_metrics(
    executor: &TeeExecutorPair, 
    key: &str, 
    execution_time_ms: Option<u64>, 
    success: bool
) {
    let mut metrics_store = executor.metrics_store.write().await;
    
    let entry = metrics_store.entry(key.to_string())
        .or_insert_with(MetricsData::default);
    
    // Update success/failure counts
    if success {
        entry.success_count += 1;
        if let Some(time) = execution_time_ms {
            // Add execution time to rolling window, removing oldest if full
            if entry.execution_times.len() >= 100 {
                entry.execution_times.pop_front();
            }
            entry.execution_times.push_back(time);
        }
    } else {
        entry.failure_count += 1;
    }
    
    // Update timestamp
    entry.last_execution = Utc::now();
}

/// Check if mesh execution should be used based on metrics
async fn should_use_mesh_execution(
    executor: &TeeExecutorPair, 
    mesh_key: &str, 
    coordinator_key: &str
) -> bool {
    let metrics_store = executor.metrics_store.read().await;
    
    // If we don't have metrics for mesh execution yet, default to trying it
    let mesh_metrics = match metrics_store.get(mesh_key) {
        Some(metrics) => metrics,
        None => return true, // No mesh metrics yet, try mesh execution
    };
    
    // If mesh has had too many failures recently, use coordinator
    if mesh_metrics.failure_count > mesh_metrics.success_count && mesh_metrics.failure_count > 5 {
        return false;
    }
    
    // Check if coordinator is faster on average (if we have data for both)
    if let Some(coord_metrics) = metrics_store.get(coordinator_key) {
        if !mesh_metrics.execution_times.is_empty() && !coord_metrics.execution_times.is_empty() {
            // Calculate average execution times
            let mesh_avg = mesh_metrics.execution_times.iter().sum::<u64>() 
                / mesh_metrics.execution_times.len() as u64;
            let coord_avg = coord_metrics.execution_times.iter().sum::<u64>() 
                / coord_metrics.execution_times.len() as u64;
            
            // Use mesh only if it's at least as fast as coordinator
            return mesh_avg <= coord_avg;
        }
    }
    
    // Default to using mesh execution if we couldn't make a decision based on metrics
    true
}

/// Check if the circuit breaker is currently tripped
async fn is_circuit_breaker_tripped(executor: &TeeExecutorPair) -> bool {
    let breaker = executor.mesh_circuit_breaker.read().await;
    
    if !breaker.is_tripped {
        return false;
    }
    
    // Check if it's time to attempt reset
    if let Some(tripped_at) = breaker.tripped_at {
        let elapsed_sec = Utc::now().signed_duration_since(tripped_at).num_seconds() as u64;
        if elapsed_sec >= breaker.reset_timeout_sec {
            // Time to reset, but we'll do that in another method
            return false;
        }
    }
    
    true
}

/// Increment failure count and possibly trip the circuit breaker
async fn increment_circuit_breaker_failures(executor: &TeeExecutorPair) -> bool {
    let mut breaker = executor.mesh_circuit_breaker.write().await;
    
    // If already tripped, do nothing
    if breaker.is_tripped {
        return true;
    }
    
    // Increment failure count
    breaker.failure_count += 1;
    
    // Check if we should trip the breaker
    if breaker.failure_count >= breaker.failure_threshold {
        breaker.is_tripped = true;
        breaker.tripped_at = Some(Utc::now());
        info!("Mesh circuit breaker tripped after {} consecutive failures", 
              breaker.failure_count);
        return true;
    }
    
    false
}

/// Reset the circuit breaker
pub async fn reset_circuit_breaker(executor: &TeeExecutorPair) {
    let mut breaker = executor.mesh_circuit_breaker.write().await;
    breaker.is_tripped = false;
    breaker.failure_count = 0;
    breaker.tripped_at = None;
    info!("Mesh circuit breaker manually reset");
}

/// Record metrics failure
async fn record_metrics_failure(executor: &TeeExecutorPair, key: &str) {
    let mut metrics_store = executor.metrics_store.write().await;
    
    let entry = metrics_store.entry(key.to_string())
        .or_insert_with(MetricsData::default);
    
    entry.failure_count += 1;
}

/// Execute a task on the coordinator
async fn execute_coordinator(executor: &TeeExecutorPair, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
    // Call the primary executor to perform the execution
    let primary = executor.primary.read().await;
    primary.execute(payload).await
}
