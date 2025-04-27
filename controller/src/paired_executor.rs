use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use chrono::Utc;
use log::{debug, error, info, warn};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

use tee_interface::{
    ExecutionPayload, ExecutionResult, ExecutionStats, TeeAttestation, TeeError, TeeExecutor, TeeType, RegionInfo
};
use crate::mesh::{MeshCoordinator, MeshExecutionResult, PeerInfo, SyncResult, TeeType as MeshTeeType};
use crate::hyper_mesh_extension::MeshExecutionExtension;

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

impl MetricsData {
    fn default() -> Self {
        Self {
            execution_times: VecDeque::with_capacity(100), // Keep last 100 execution times
            success_count: 0,
            failure_count: 0,
            last_execution: Utc::now(),
        }
    }
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
                                enclave_type: match a.enclave_type.to_lowercase().as_str() {
                                    "sgx" | "intelsgx" => tee_interface::TeeType::SGX,
                                    "sev" => tee_interface::TeeType::SEV,
                                    _ => tee_interface::TeeType::SGX,
                                },
                                measurement: a.measurement.clone(),
                                timestamp: a.timestamp,
                                data: a.platform_data.clone(),
                                enclave_id: Vec::new(),
                                signature: Vec::new(),
                                region_proof: None,
                            })
                            .collect();
                        
                        // Calculate total execution time
                        let execution_time = Utc::now().signed_duration_since(start_time).num_milliseconds() as u64;
                        
                        // Update metrics with successful mesh execution data
                        let mut stats = tee_interface::ExecutionStats {
                            execution_time,
                            memory_used: mesh_result.memory_used,
                            syscall_count: mesh_result.syscall_count,
                            network_latency: mesh_result.network_latency_ns,
                            custom_metrics: Some(HashMap::new()),
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

#[async_trait]
impl MeshExecutionExtension for TeeExecutorPair {
    async fn try_mesh_execution(&self, payload: &ExecutionPayload) -> Result<Option<ExecutionResult>, TeeError> {
        // Don't attempt mesh execution if mesh coordinator is not available
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator,
            None => {
                debug!("Mesh coordinator not available, skipping mesh execution");
                return Ok(None);
            }
        };

        // Extract target information
        let region_id = payload.region_id.as_deref().unwrap_or("default");
        let target_tee = match &payload.target_tee {
            Some(tee) => tee.as_str(),
            None => {
                debug!("No target TEE specified, cannot use mesh execution");
                return Ok(None);
            }
        };
        
        // Determine TEE type, default to "sgx" if not specified
        let tee_type = payload.tee_type.as_deref().unwrap_or("sgx");

        // Check if circuit breaker is tripped for this target
        {
            let circuit_breaker = self.mesh_circuit_breaker.read().await;
            if circuit_breaker.is_tripped {
                warn!("Circuit breaker tripped for mesh execution, skipping");
                return Ok(None);
            }
        }

        // Check if we should attempt mesh execution based on metrics and payload
        if !self.should_attempt_mesh(payload).await {
            debug!("Mesh execution not recommended based on metrics, skipping");
            return Ok(None);
        }

        // Default mesh timeout (100ms target for sub-100ms communication)
        let mesh_timeout = Duration::from_millis(100);
        
        // Execute via mesh network
        debug!("Attempting mesh execution for {}/{}", region_id, target_tee);
        let start_time = Instant::now();
        
        // Call the execute method with the correct parameters
        match mesh_coordinator.execute(
            target_tee.to_string(),
            region_id.to_string(),
            tee_type.to_string(),
            payload.input.clone(),
            mesh_timeout,
            false, // synchronous execution for now
            payload.allow_fallback.unwrap_or(true),
        ).await {
            Ok(mesh_result) => {
                let duration = start_time.elapsed();
                debug!("Mesh execution successful for {}/{} in {:?}", region_id, target_tee, duration);
                
                // Convert mesh result to execution result
                let result = self.convert_mesh_result(mesh_result, payload);
                
                // Record metrics for successful mesh execution
                self.record_mesh_success(region_id, target_tee, duration).await;
                
                // Return the execution result
                Ok(Some(result))
            },
            Err(e) => {
                let duration = start_time.elapsed();
                warn!("Mesh execution failed for {}/{}: {} (after {:?})", 
                     region_id, target_tee, e, duration);
                
                // Update circuit breaker
                {
                    let mut circuit_breaker = self.mesh_circuit_breaker.write().await;
                    circuit_breaker.failure_count += 1;
                    if circuit_breaker.failure_count >= circuit_breaker.failure_threshold {
                        circuit_breaker.is_tripped = true;
                        circuit_breaker.tripped_at = Some(Utc::now());
                    }
                }
                
                // If we failed quickly, we can try coordinator execution
                // This supports our sub-100ms performance target by quickly falling back
                if duration < Duration::from_millis(50) {
                    debug!("Mesh execution failed quickly, will try coordinator");
                    Ok(None)
                } else {
                    // If we've already spent significant time, propagate the error
                    let region_id = payload.region_id.as_deref().unwrap_or("unknown");
                    let target_tee = payload.target_tee.as_deref().unwrap_or("unknown");
                    
                    // Record the mesh failure
                    self.record_mesh_failure(region_id, target_tee).await;
                    
                    // Return the original error as TeeError
                    Err(TeeError::ExecutionError(format!("{}", e)))
                }
            }
        }
    }
    
    async fn should_attempt_mesh(&self, payload: &ExecutionPayload) -> bool {
        // Check if mesh is enabled via coordinator presence
        if self.mesh_coordinator.is_none() {
            return false;
        }
        
        // Check if the payload allows fallback
        if payload.allow_fallback.unwrap_or(true) == false {
            // If fallback is not allowed, we must use mesh execution
            return true;
        }
        
        // Check if target_tee is specified
        if payload.target_tee.is_none() {
            return false;
        }
        
        // Get metrics to make the decision
        let metrics = self.metrics_store.read().await;
        
        // Extract region and target TEE
        let region_id = payload.region_id.as_deref().unwrap_or("default");
        let target_tee = match &payload.target_tee {
            Some(tee) => tee,
            None => return false,
        };
        
        // Look up metrics for this target
        let metric_key = format!("{}/{}", region_id, target_tee);
        if let Some(metrics_data) = metrics.get(&metric_key) {
            // Check if mesh execution is historically faster
            if metrics_data.is_mesh_faster_than_coordinator() {
                return true;
            }
            
            // Check if mesh execution has been reliable
            if metrics_data.get_mesh_success_rate() > 0.9 {  // 90% success rate threshold
                return true;
            }
        }
        
        // Default to using mesh if we have no metrics (experimental)
        // This helps us gather metrics for new targets
        true
    }
    
    fn convert_mesh_result(&self, mesh_result: MeshExecutionResult, payload: &ExecutionPayload) -> ExecutionResult {
        // Map MeshExecutionResult fields to ExecutionResult fields
        let mut custom_metrics = HashMap::new();
        
        // Add mesh-specific metrics to custom_metrics
        custom_metrics.insert("status".to_string(), mesh_result.status.clone());
        custom_metrics.insert("cache_hit".to_string(), mesh_result.cache_hit.to_string());
        custom_metrics.insert("execution_type".to_string(), mesh_result.execution_type.clone());
        custom_metrics.insert("network_latency_ns".to_string(), mesh_result.network_latency_ns.to_string());
        
        // Add optional worker metrics if available
        if let Some(metrics) = &mesh_result.metrics {
            custom_metrics.insert("worker_id".to_string(), metrics.worker_id.clone());
            custom_metrics.insert("region_id".to_string(), metrics.region_id.clone());
            custom_metrics.insert("tee_type".to_string(), metrics.tee_type.clone());
        }
        
        ExecutionResult {
            result: mesh_result.result,
            state_hash: mesh_result.state_hash,
            stats: ExecutionStats {
                execution_time: mesh_result.execution_time_ns,
                memory_used: mesh_result.memory_used,
                syscall_count: mesh_result.syscall_count,
                network_latency: mesh_result.network_latency_ns,
                custom_metrics: Some(custom_metrics),
            },
            attestations: mesh_result.attestations
                .unwrap_or_default()
                .into_iter()
                .map(|a| TeeAttestation {
                    enclave_type: match a.enclave_type.to_lowercase().as_str() {
                        "sgx" | "intelsgx" => tee_interface::TeeType::SGX,
                        "sev" => tee_interface::TeeType::SEV,
                        _ => tee_interface::TeeType::SGX, // Default to SGX if unknown
                    },
                    measurement: a.measurement.clone(),
                    timestamp: a.timestamp,
                    data: a.platform_data.clone(),
                    enclave_id: Vec::new(),
                    signature: Vec::new(),
                    region_proof: None,
                })
                .collect(),
            timestamp: Utc::now().to_rfc3339(),
            operation_status: Some("completed".to_string()),
            operation_id: Some(mesh_result.operation_id.unwrap_or_else(|| 
                payload.operation_id.clone().unwrap_or_else(|| Utc::now().timestamp_nanos_opt().unwrap_or(0).to_string()))),
            pending_operations: Some(Vec::new()),
        }
    }
    
    async fn record_mesh_failure(&self, payload: &ExecutionPayload, error_msg: &str) -> TeeError {
        // Log the failure for metrics purposes
        warn!("Mesh execution failed for operation {}: {}", 
              payload.operation_id.as_deref().unwrap_or("unknown"), error_msg);
        
        // Create and return a formatted error
        TeeError::ExecutionError(format!(
            "Mesh execution failed: {}. Operation ID: {}", 
            error_msg,
            payload.operation_id.as_deref().unwrap_or("unknown")
        ))
    }
}

impl TeeExecutorPair {
    // Helper method to record successful mesh execution metrics
    async fn record_mesh_success(&self, region_id: &str, target_tee: &str, duration: Duration) {
        let mut metrics = self.metrics_store.write().await;
        let metric_key = format!("{}/{}", region_id, target_tee);
        
        let metrics_data = metrics.entry(metric_key.clone()).or_insert_with(|| MetricsData::default());
        
        // Record successful execution
        metrics_data.execution_times.push_back(duration.as_millis() as u64);
        metrics_data.success_count += 1;
        metrics_data.last_execution = Utc::now();
        
        // Keep only the most recent measurements
        if metrics_data.execution_times.len() > 100 {
            metrics_data.execution_times.pop_front();
        }
        
        // Reset circuit breaker for successful executions
        {
            let mut circuit_breaker = self.mesh_circuit_breaker.write().await;
            circuit_breaker.is_tripped = false;
            circuit_breaker.failure_count = 0;
        }
    }
    
    // Helper method to record failed mesh execution metrics
    async fn record_mesh_failure(&self, region_id: &str, target_tee: &str) {
        let mut metrics = self.metrics_store.write().await;
        let metric_key = format!("{}/{}", region_id, target_tee);

        let metrics_data = metrics.entry(metric_key.clone()).or_insert_with(|| MetricsData::default());

        // Record failed execution
        metrics_data.failure_count += 1;
        metrics_data.last_execution = Utc::now();
    }
}

/// Metrics data structure to help with routing decisions
impl MetricsData {
    // Check if mesh execution is historically faster than coordinator
    fn is_mesh_faster_than_coordinator(&self) -> bool {
        // This would compare mesh execution times with coordinator times
        // For now, use a simplified heuristic
        if self.execution_times.len() < 5 {
            return false; // Not enough data
        }
        
        // Calculate average execution time
        let sum: u64 = self.execution_times.iter().sum();
        let avg = sum / self.execution_times.len() as u64;
        
        // If average execution time is under 50ms, prefer mesh
        avg < 50
    }
    
    // Get success rate for mesh execution
    fn get_mesh_success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 1.0; // No data yet, assume perfect
        }
        self.success_count as f64 / total as f64
    }
}

/// Circuit breaker state to prevent repeated failures
impl CircuitBreakerState {
    fn is_tripped(&self) -> bool {
        self.is_tripped
    }
}

/// Update performance metrics for a specific execution path
async fn update_metrics(
    executor: &TeeExecutorPair, 
    key: &str, 
    execution_time_ms: Option<u64>, 
    success: bool
) {
    let mut metrics_store = executor.metrics_store.write().await;
    
    let entry = metrics_store.entry(key.to_string())
        .or_insert_with(|| MetricsData::default());
    
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
        .or_insert_with(|| MetricsData::default());
    
    entry.failure_count += 1;
}

/// Execute a task on the coordinator
async fn execute_coordinator(executor: &TeeExecutorPair, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
    // Call the primary executor to perform the execution
    let primary = executor.primary.read().await;
    primary.execute(payload).await
}
