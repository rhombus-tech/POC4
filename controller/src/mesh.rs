// Module for implementing mesh network functionality in the TEE controller
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time;
use log::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use rand::Rng;
use tokio::sync::Semaphore;
use uuid::Uuid;

// Define TeeType enum for mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeeType {
    /// Intel SGX
    SGX,
    /// AMD SEV
    SEV,
}

impl ToString for TeeType {
    fn to_string(&self) -> String {
        match self {
            TeeType::SGX => "SGX".to_string(),
            TeeType::SEV => "SEV".to_string(),
        }
    }
}

// Implement FromStr for TeeType for string parsing
impl std::str::FromStr for TeeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "SGX" => Ok(TeeType::SGX),
            "SEV" => Ok(TeeType::SEV),
            _ => Err(format!("Unknown TEE type: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub region_id: String,
    pub endpoint: String,
    pub tee_id: String,
    pub max_peers: usize,
    pub discovery_interval_sec: u64,
    pub discovery_endpoint: String,
    pub circuit_breaker_threshold: Duration,
    pub peer_refresh_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub tee_id: String,
    pub tee_type: String,
    pub region_id: String,
    pub endpoint: String,
    pub status: String,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub status: String,
    pub bytes_transferred: u64,
    pub sync_time_ms: u64,
    pub objects_synced: u64,
    pub deltas_used: bool,
    pub target_tee: String,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshExecutionParams {
    pub target_tee: String,
    pub region_id: String,
    pub tee_type: String,
    pub timeout: Duration,
    pub is_async: bool,
    pub allow_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshExecutionResult {
    pub result: Vec<u8>,
    pub execution_time_ns: u64,
    pub network_latency_ns: u64,
    pub attestations: Option<Vec<Attestation>>,
    pub error: Option<String>,
    pub metrics: Option<PerformanceMetrics>,
    pub state_hash: Vec<u8>,
    pub memory_used: u64,
    pub syscall_count: u64,
    pub status: String,
    pub cache_hit: bool,
    pub cache_ttl_sec: Option<u64>,
    pub execution_type: String,
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub tee_type: String,
    pub region_id: String,
    pub worker_id: String,
    pub latency_ms: f64,
    pub execution_time_ns: u64,
    pub network_latency_ms: f64,
    pub success_count: u64,
    pub failure_count: u64,
    pub memory_used_bytes: u64,
    pub syscall_count: u64,
    pub throughput_bytes_ps: u64,
    pub p50_execution_ms: Option<u64>,
    pub p95_execution_ms: Option<u64>,
    pub p99_execution_ms: Option<u64>,
    pub max_execution_ms: Option<u64>,
    pub avg_execution_ms: Option<f64>,
    pub min_execution_ms: Option<u64>,
    pub operations_per_second: Option<u64>,
    pub batch_size: Option<u64>, 
    pub concurrent_operations: Option<u64>,
    pub network_efficiency: Option<f64>, // Ratio of execution time to network latency
}

// Struct to track peer status and connection info
#[derive(Debug, Clone)]
struct PeerState {
    tee_id: String,
    tee_type: String,
    region_id: String,
    endpoint: String,
    status: String,
    last_ping: std::time::Instant,
    latency_history: Vec<Duration>,
    success_count: u64,
    failure_count: u64,
}

// Cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    result: MeshExecutionResult,
    expiry: std::time::Instant,
}

// Batch operation for more efficient mesh execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    pub target_tee: String,
    pub region_id: String,
    pub tee_type: String,
    pub input: Vec<u8>,
    pub operation_id: String,
}

// Batch execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchExecutionResult {
    pub batch_id: String,
    pub operations: Vec<MeshExecutionResult>,
    pub total_execution_time_ms: u64,
    pub batch_attestation: Option<Attestation>,
    pub performance: Option<PerformanceMetrics>,
}

// Connection information for a peer
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub peer_id: String,
    pub region_id: String,
    pub tee_type: TeeType,
    pub endpoint: String,
    pub created_at: std::time::Instant,
    pub last_used: std::time::Instant,
    pub use_count: u64,
    pub failed_attempts: u32,
    pub is_healthy: bool,
    pub latency_ms: f64,
}

// Connection pool for managing persistent connections to peers
#[derive(Debug)]
pub struct ConnectionPool {
    // Map of connection key (region:tee_type:peer_id) to connection info
    connections: RwLock<HashMap<String, ConnectionInfo>>,
    // Maximum number of connections to maintain in the pool
    max_connections: usize,
    // Connection timeout
    connection_timeout: Duration,
    // Connection idle timeout - how long to keep unused connections
    idle_timeout: Duration,
    // Maximum failures before marking a connection as unhealthy
    max_failures: u32,
    // Semaphore to limit concurrent connection creation
    connection_semaphore: Arc<Semaphore>,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        ConnectionPool {
            connections: RwLock::new(HashMap::new()),
            max_connections,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            max_failures: 3,
            connection_semaphore: Arc::new(Semaphore::new(10)), // Limit concurrent connection creation
        }
    }
    
    // Get a connection for a specific peer, region, and TEE type
    pub fn get_connection(&self, peer_id: &str, region_id: &str, tee_type: &TeeType) -> Option<ConnectionInfo> {
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        let mut connections = self.connections.write().unwrap();
        
        if let Some(connection) = connections.get_mut(&key) {
            // Update last used time
            connection.last_used = std::time::Instant::now();
            connection.use_count += 1;
            
            if connection.is_healthy {
                Some(connection.clone())
            } else {
                None
            }
        } else {
            None
        }
    }
    
    // Create a new connection to a peer
    pub fn create_connection(&self, peer_id: &str, region_id: &str, tee_type: &TeeType, endpoint: &str) -> ConnectionInfo {
        let now = std::time::Instant::now();
        let connection = ConnectionInfo {
            peer_id: peer_id.to_string(),
            region_id: region_id.to_string(),
            tee_type: *tee_type,
            endpoint: endpoint.to_string(),
            created_at: now,
            last_used: now,
            use_count: 1,
            failed_attempts: 0,
            is_healthy: true,
            latency_ms: 0.0,
        };
        
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        let mut connections = self.connections.write().unwrap();
        
        // If we're at the connection limit, remove the oldest unused connection
        if connections.len() >= self.max_connections {
            let now = std::time::Instant::now();
            if let Some((oldest_key, _)) = connections.iter()
                .filter(|(_, conn)| now.duration_since(conn.last_used) > self.idle_timeout)
                .min_by_key(|(_, conn)| conn.last_used) {
                    let oldest_key = oldest_key.clone();
                    connections.remove(&oldest_key);
                    debug!("Removed idle connection: {}", oldest_key);
                }
        }
        
        connections.insert(key, connection.clone());
        connection
    }
    
    // Mark a connection as failed
    pub fn mark_connection_failed(&self, peer_id: &str, region_id: &str, tee_type: &TeeType) {
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        let mut connections = self.connections.write().unwrap();
        
        if let Some(connection) = connections.get_mut(&key) {
            connection.failed_attempts += 1;
            if connection.failed_attempts >= self.max_failures {
                connection.is_healthy = false;
                warn!("Connection marked as unhealthy: {}", key);
            }
        }
    }
    
    // Mark a connection as healthy after successful use
    pub fn mark_connection_healthy(&self, peer_id: &str, region_id: &str, tee_type: &TeeType, latency_ms: f64) {
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        let mut connections = self.connections.write().unwrap();
        
        if let Some(connection) = connections.get_mut(&key) {
            connection.is_healthy = true;
            connection.failed_attempts = 0;
            connection.latency_ms = latency_ms;
        }
    }
    
    // Clean up idle connections
    pub fn cleanup_idle_connections(&self) -> usize {
        let mut connections = self.connections.write().unwrap();
        let now = std::time::Instant::now();
        
        let idle_keys: Vec<String> = connections.iter()
            .filter(|(_, conn)| now.duration_since(conn.last_used) > self.idle_timeout)
            .map(|(key, _)| key.clone())
            .collect();
        
        let count = idle_keys.len();
        for key in idle_keys {
            connections.remove(&key);
            debug!("Removed idle connection: {}", key);
        }
        
        count
    }
    
    // Get total connection count
    pub fn connection_count(&self) -> usize {
        let connections = self.connections.read().unwrap();
        connections.len()
    }
    
    // Get healthy connection count
    pub fn healthy_connection_count(&self) -> usize {
        let connections = self.connections.read().unwrap();
        connections.iter().filter(|(_, conn)| conn.is_healthy).count()
    }
    
    // Clone for use in async contexts
    pub fn clone(&self) -> Self {
        ConnectionPool {
            connections: RwLock::new(self.connections.read().unwrap().clone()),
            max_connections: self.max_connections,
            connection_timeout: self.connection_timeout,
            idle_timeout: self.idle_timeout,
            max_failures: self.max_failures,
            connection_semaphore: self.connection_semaphore.clone(),
        }
    }
}

// Mesh coordinator
pub struct MeshCoordinator {
    config: MeshConfig,
    peers: RwLock<HashMap<String, PeerInfo>>,
    cache: RwLock<HashMap<String, CacheEntry>>,
    local_state: RwLock<HashMap<String, Vec<u8>>>,
    max_concurrent_executions: Semaphore,
    connection_pool: Arc<ConnectionPool>,
}

impl MeshCoordinator {
    pub async fn new(config: MeshConfig) -> Result<Self, std::io::Error> {
        let max_peers = config.max_peers; // Save max_peers before moving config
        let coordinator = MeshCoordinator {
            config,
            peers: RwLock::new(HashMap::new()),
            cache: RwLock::new(HashMap::new()),
            local_state: RwLock::new(HashMap::new()),
            max_concurrent_executions: Semaphore::new(max_peers),
            connection_pool: Arc::new(ConnectionPool::new(100)), // Support up to 100 connections
        };
        
        // Initialize and start peer discovery
        coordinator.start_discovery().await?;
        
        Ok(coordinator)
    }
    
    async fn start_discovery(&self) -> Result<(), std::io::Error> {
        info!("Initializing mesh coordinator for TEE ID: {}", self.config.tee_id);
        
        // Start periodic peer discovery
        let config_clone = self.config.clone();
        let coordinator = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(config_clone.discovery_interval_sec));
            loop {
                interval.tick().await;
                if let Err(e) = coordinator.refresh_peers().await {
                    warn!("Failed to refresh peers: {:?}", e);
                }
            }
        });
        
        // Perform initial peer discovery
        self.refresh_peers().await?;
        
        Ok(())
    }
    
    async fn refresh_peers(&self) -> Result<(), std::io::Error> {
        debug!("Refreshing peers for region: {}", self.config.region_id);
        
        // In a real implementation, this would call the discovery endpoint
        // For now, just simulate peer discovery
        
        Ok(())
    }
    
    // Execute a workload on a specific TEE in the mesh
    pub async fn execute(
        &self,
        target_tee: String,
        region_id: String,
        tee_type_str: String,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        // Parse the TeeType from string
        let tee_type = match tee_type_str.parse::<TeeType>() {
            Ok(t) => t,
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Invalid TEE type: {}", e)
                ));
            }
        };

        info!("Executing via mesh network in region: {}, target: {}, tee_type: {}", 
             region_id, target_tee, tee_type.to_string());
        
        let start_time = std::time::Instant::now();
        
        // First check if we have a pooled connection
        let connection = self.connection_pool.get_connection(&target_tee, &region_id, &tee_type);
        
        // Find appropriate peer in network if no connection exists
        let peer_endpoint = if let Some(connection) = &connection {
            connection.endpoint.clone()
        } else {
            let peers = self.peers.read().unwrap();
            let peer_key = format!("{}:{}", region_id, tee_type.to_string());
            
            if let Some(peer) = peers.get(&peer_key) {
                // Found a peer, use its endpoint
                peer.endpoint.clone()
            } else {
                // No peer found, return error or use fallback
                if allow_fallback {
                    // Just use a simulated endpoint for now
                    format!("https://{}.{}.example.com", target_tee, region_id)
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("No peer found for {} in region {}", tee_type.to_string(), region_id)
                    ));
                }
            }
        };
        
        // Simulate actual network communication with connection management
        let execution_start = std::time::Instant::now();
        
        // If we have a connection, use it, otherwise create a new one
        let mut conn_info = connection.unwrap_or_else(|| {
            self.connection_pool.create_connection(&target_tee, &region_id, &tee_type, &peer_endpoint)
        });
        
        // Simulate execution time based on connection quality
        let execution_time = if conn_info.is_healthy {
            // Faster execution for healthy connections
            Duration::from_millis(10 + (5))
        } else {
            // Slower execution for new or recovering connections
            Duration::from_millis(10 + (15))
        };
        
        // Simulate network latency based on connection history
        let network_latency_ms = if conn_info.latency_ms > 0.0 {
            // Use historical latency with some variance
            let mut rng = rand::thread_rng();
            let variance = rng.gen_range(-2.0..2.0);
            (conn_info.latency_ms + variance).max(1.0)
        } else {
            // New connection, use higher initial latency
            10.0
        };
        
        // Simulate the actual execution and network delay
        time::sleep(execution_time).await;
        time::sleep(Duration::from_millis(network_latency_ms as u64)).await;
        
        // Update connection stats after successful execution
        self.connection_pool.mark_connection_healthy(
            &target_tee, 
            &region_id, 
            &tee_type, 
            network_latency_ms
        );
        
        // Generate a random sha256 hash for this execution
        let mut rng = rand::thread_rng();
        let random_hash: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
        
        let total_latency = start_time.elapsed();
        let network_latency = total_latency.checked_sub(execution_time).unwrap_or_default();
        
        // Generate attestation
        let attestation = Attestation {
            enclave_type: tee_type.to_string(),
            measurement: random_hash.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            platform_data: vec![0, 1, 2, 3],
        };
        
        // Create performance metrics
        let metrics = PerformanceMetrics {
            tee_type: tee_type.to_string(),
            region_id: region_id.clone(),
            worker_id: target_tee.clone(),
            latency_ms: network_latency_ms,
            execution_time_ns: execution_time.as_nanos() as u64,
            network_latency_ms: network_latency_ms,
            success_count: 1,
            failure_count: 0,
            memory_used_bytes: 1024 * 1024, // 1MB example
            syscall_count: 42,
            throughput_bytes_ps: (input.len() as u64 * 1000) / 
                (execution_time.as_millis() as u64).max(1),
            p50_execution_ms: None,
            p95_execution_ms: None,
            p99_execution_ms: None,
            max_execution_ms: None,
            avg_execution_ms: None,
            min_execution_ms: None,
            operations_per_second: None,
            batch_size: None,
            concurrent_operations: None,
            network_efficiency: Some(conn_info.use_count as f64 / (conn_info.failed_attempts as f64 + 1.0)),
        };
        
        let result = MeshExecutionResult {
            result: input, // Echo input as a simulated result for now
            execution_time_ns: execution_time.as_nanos() as u64,
            network_latency_ns: network_latency.as_nanos() as u64,
            attestations: Some(vec![attestation]),
            error: None,
            metrics: Some(metrics),
            state_hash: random_hash,
            memory_used: 1024 * 1024, // 1MB example
            syscall_count: 42,
            status: "completed".to_string(),
            cache_hit: false,
            cache_ttl_sec: None,
            execution_type: "normal".to_string(),
            operation_id: None,
        };
        
        // Add execution time to metrics
        let elapsed = start_time.elapsed();
        info!("Mesh execution completed in {:?}", elapsed);
        
        Ok(result)
    }
    
    // Execute with cache support
    pub async fn execute_with_cache(
        &self,
        target_tee: String,
        region_id: String,
        tee_type: String,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
        use_cache: bool,
        cache_ttl: Duration,
        stale_result_timeout: Duration,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        info!("Executing with mesh cache: target={}, region={}, use_cache={}", 
              target_tee, region_id, use_cache);
        
        // Generate cache key
        let cache_key = self.generate_cache_key(&target_tee, &region_id, &tee_type, &input);
        
        // Check cache if requested
        if use_cache {
            if let Some(cached_result) = self.check_cache(&cache_key, stale_result_timeout) {
                debug!("Cache hit for key: {}", cache_key);
                let mut result = cached_result.clone();
                result.cache_hit = true;
                return Ok(result);
            }
        }
        
        // If not in cache, execute normally
        let mut result = self.execute(
            target_tee.clone(), 
            region_id.clone(), 
            tee_type, 
            input.clone(), 
            timeout, 
            is_async, 
            allow_fallback
        ).await?;
        
        // Update cache TTL in the result
        result.cache_ttl_sec = Some(cache_ttl.as_secs());
        
        // Store in cache if requested
        if use_cache {
            self.store_in_cache(&cache_key, &result, cache_ttl);
        }
        
        Ok(result)
    }
    
    pub async fn execute_paired(
        &self,
        target_tee: String,
        region_id: String,
        tee_type: String,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        info!("Executing paired execution: target={}, region={}, type={}, use_cache={}", 
              target_tee, region_id, tee_type, false);
        
        // Generate cache key
        let cache_key = self.generate_cache_key(&target_tee, &region_id, &tee_type, &input);
        
        // Check cache if requested
        if let Some(cached_result) = self.check_cache(&cache_key, timeout) {
            info!("Using cached result for paired execution");
            return Ok(cached_result);
        }
        
        // Execute the request
        let mut result = self.execute(
            target_tee.clone(), 
            region_id.clone(), 
            tee_type, 
            input.clone(), 
            timeout, 
            is_async, 
            allow_fallback
        ).await?;
        
        // Mark as paired execution
        result.execution_type = "paired".to_string();
        
        // Store in cache with TTL
        self.store_in_cache(&cache_key, &result, Duration::from_secs(3600));
        
        Ok(result)
    }
    
    // Generate a cache key
    fn generate_cache_key(&self, target_tee: &str, region_id: &str, tee_type: &str, input: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(target_tee.as_bytes());
        hasher.update(region_id.as_bytes());
        hasher.update(tee_type.as_bytes());
        hasher.update(input);
        
        let hash = hasher.finalize();
        format!("mesh:exec:{}:{}:{}:{}", target_tee, region_id, tee_type, hash.iter().map(|b| format!("{:02x}", b)).collect::<String>())
    }
    
    // Check the cache for a result
    fn check_cache(&self, cache_key: &str, _stale_result_timeout: Duration) -> Option<MeshExecutionResult> {
        // Try to read from cache
        if let Ok(cache) = self.cache.read() {
            if let Some(entry) = cache.get(cache_key) {
                debug!("Cache hit for key: {}", cache_key);
                return Some(entry.result.clone());
            }
        } else {
            warn!("Failed to acquire read lock on cache");
        }
        
        debug!("Cache miss for key: {}", cache_key);
        None
    }
    
    fn store_in_cache(&self, cache_key: &str, result: &MeshExecutionResult, ttl: Duration) {
        // Store results in our in-memory cache
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(cache_key.to_string(), CacheEntry {
                result: result.clone(),
                expiry: std::time::Instant::now() + ttl,
            });
            debug!("Storing result in cache with key {} and TTL of {} seconds", 
                   cache_key, ttl.as_secs());
        } else {
            error!("Failed to acquire write lock for cache");
        }
    }
    
    // Discover peers in the mesh network
    pub async fn discover_peers(
        &self,
        region_id: String,
        tee_type: Option<String>,
        max_results: usize,
    ) -> Result<Vec<PeerInfo>, std::io::Error> {
        let peers = self.peers.read().unwrap();
        
        let filtered_peers: Vec<PeerInfo> = peers.values()
            .filter(|p| p.region_id == region_id)
            .filter(|p| tee_type.clone().map_or(true, |t| p.tee_type == t))
            .take(max_results)
            .map(|p| PeerInfo {
                tee_id: p.tee_id.clone(),
                tee_type: p.tee_type.clone(),
                region_id: p.region_id.clone(),
                endpoint: p.endpoint.clone(),
                status: p.status.clone(),
                latency_ms: p.latency_ms, // Use existing latency_ms field
            })
            .collect();
        
        // If no peers are found, create simulated peers for testing
        if filtered_peers.is_empty() {
            let mut simulated_peers = Vec::new();
            for i in 0..3.min(max_results) {
                let tee_type_value = if let Some(t) = tee_type.clone() {
                    t
                } else if i % 2 == 0 {
                    "SGX".to_string()
                } else {
                    "SEV".to_string()
                };
                
                simulated_peers.push(PeerInfo {
                    tee_id: format!("tee-{}", Uuid::new_v4()),
                    tee_type: tee_type_value,
                    region_id: region_id.clone(),
                    endpoint: format!("localhost:5005{}", i),
                    status: "active".to_string(),
                    latency_ms: 5.0 + (i as f64) * 10.0,
                });
            }
            
            return Ok(simulated_peers);
        }
        
        Ok(filtered_peers)
    }
    
    // Synchronize state with another TEE
    pub async fn sync_state(
        &self,
        object_id: String,
        target_tee: String,
        use_deltas: bool,
    ) -> Result<SyncResult, std::io::Error> {
        info!("Syncing state with TEE: {}, object: {}", target_tee, object_id);
        
        let start_time = std::time::Instant::now();
        
        // In a real implementation, we would:
        // 1. Connect to the target TEE
        // 2. Request its current state hash for the object
        // 3. Compare with our local state
        // 4. If different and use_deltas, compute and send only the differences
        // 5. Otherwise, send the full state
        
        // For now, simulate a successful sync
        // This will be replaced with actual TEE-to-TEE state sync
        
        let state_data: Vec<u8> = Vec::new();
        
        // Simulated result
        let result = SyncResult {
            status: "success".to_string(),
            bytes_transferred: 1024,
            sync_time_ms: 50,
            objects_synced: 1,
            deltas_used: use_deltas,
            target_tee: target_tee.clone(),
            object_ids: vec![object_id.clone()],
        };
        
        // Log sync time
        let elapsed = start_time.elapsed();
        info!("State sync completed in {:?}", elapsed);
        
        Ok(result)
    }
    
    pub async fn try_executor(
        &self,
        target_tee: String,
        region_id: String,
        tee_type: String,
        input: Vec<u8>,
        timeout: Duration,
        is_async: bool,
        allow_fallback: bool,
    ) -> Result<MeshExecutionResult, std::io::Error> {
        // Generate a cache key for this execution
        let cache_key = self.generate_cache_key(&target_tee, &region_id, &tee_type, &input);
        
        // Check if we already have a cached result
        if let Some(cached_result) = self.check_cache(&cache_key, timeout) {
            info!("Using cached result for execution in region {}, TEE {}", region_id, target_tee);
            return Ok(cached_result);
        }
        
        // Create execution semaphore
        let permit = match self.max_concurrent_executions.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                if !allow_fallback {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ResourceBusy,
                        "Maximum concurrent executions reached",
                    ));
                }
                
                // If fallback is allowed, check if we have a cached result with a more lenient timeout
                if let Some(stale_result) = self.check_cache(&cache_key, timeout * 10) {
                    warn!("Using stale cached result due to resource constraints");
                    return Ok(stale_result);
                }
                
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ResourceBusy,
                    "Maximum concurrent executions reached and no suitable cached result found",
                ));
            }
        };
        
        // Execute the request
        let result = self.execute(
            target_tee.clone(), 
            region_id.clone(), 
            tee_type, 
            input.clone(), 
            timeout, 
            is_async, 
            allow_fallback
        ).await;
        
        // Store result in cache if successful
        if let Ok(ref exec_result) = result {
            self.store_in_cache(&cache_key, exec_result, Duration::from_secs(3600));
        }
        
        // Drop semaphore permit
        drop(permit);
        
        result
    }
    
    // Execute a batch of operations for higher throughput
    pub async fn execute_batch(
        &self,
        operations: Vec<BatchOperation>,
        timeout: Duration,
        allow_fallback: bool,
    ) -> Result<BatchExecutionResult, std::io::Error> {
        if operations.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Batch cannot be empty"
            ));
        }

        let batch_id = Uuid::new_v4().to_string();
        let batch_size = operations.len();
        info!("Executing batch {} with {} operations", batch_id, batch_size);
        
        let start_time = std::time::Instant::now();
        
        // Group operations by target TEE for locality-aware execution
        let mut operations_by_target: HashMap<String, Vec<BatchOperation>> = HashMap::new();
        
        for op in operations {
            let target_key = format!("{}:{}:{}", op.region_id, op.target_tee, op.tee_type);
            operations_by_target
                .entry(target_key)
                .or_insert_with(Vec::new)
                .push(op);
        }
        
        // Increase max concurrency for higher throughput
        let max_concurrent = std::cmp::min(operations_by_target.len() * 4, 32);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        
        let mut results = Vec::new();
        let mut handles = Vec::new();
        
        // Process each target in parallel with higher concurrency
        for (target, target_operations) in operations_by_target {
            let op = &target_operations[0];
            let target_id = op.target_tee.clone();
            let region_id = op.region_id.clone();
            let tee_type_str = op.tee_type.clone();
            
            // Parse the TeeType from string
            let tee_type = match tee_type_str.parse::<TeeType>() {
                Ok(t) => t,
                Err(e) => {
                    error!("Invalid TEE type: {}", e);
                    continue;
                }
            };
            
            // Check if we have a pooled connection for this target
            let connection = self.connection_pool.get_connection(&target_id, &region_id, &tee_type);
            
            // Find peer endpoint if needed
            let peer_endpoint = if let Some(connection) = &connection {
                connection.endpoint.clone()
            } else {
                let peers = self.peers.read().unwrap();
                let peer_key = format!("{}:{}", region_id, tee_type.to_string());
                
                if let Some(peer) = peers.get(&peer_key) {
                    // Found a peer, use its endpoint
                    peer.endpoint.clone()
                } else {
                    // No peer found, use fallback if allowed
                    if allow_fallback {
                        format!("https://{}.{}.example.com", target_id, region_id)
                    } else {
                        error!("No peer found for {} in region {}", tee_type.to_string(), region_id);
                        continue;
                    }
                }
            };
            
            // Create a connection if needed
            if connection.is_none() {
                self.connection_pool.create_connection(&target_id, &region_id, &tee_type, &peer_endpoint);
            }
            
            // Create a chunk size based on the number of operations per target
            // Smaller chunks for better parallelism
            let chunk_size = std::cmp::min(10, (target_operations.len() + 9) / 10);
            
            // Execute operations in parallel chunks for this target
            for chunk in target_operations.chunks(chunk_size) {
                let chunk = chunk.to_vec();
                let self_clone = self.clone();
                let semaphore_clone = semaphore.clone();
                // Clone target info for each chunk
                let target_id_clone = target_id.clone();
                let region_id_clone = region_id.clone();
                let tee_type_clone = tee_type;
                
                let handle = tokio::spawn(async move {
                    let _permit = semaphore_clone.acquire().await.unwrap();
                    
                    // Get connection for this chunk
                    let connection = self_clone.connection_pool.get_connection(
                        &target_id_clone, 
                        &region_id_clone, 
                        &tee_type_clone
                    );
                    
                    let mut chunk_results = Vec::new();
                    let mut chunk_tasks = Vec::new();
                    
                    // Execute operations in this chunk with connection reuse
                    for op in chunk {
                        let self_clone2 = self_clone.clone();
                        let target_id = op.target_tee.clone();
                        let region_id = op.region_id.clone();
                        let tee_type = op.tee_type.clone();
                        let input = op.input.clone();
                        let operation_id = op.operation_id.clone();
                        
                        let task = tokio::spawn(async move {
                            let result = self_clone2.execute(
                                target_id.clone(),
                                region_id.clone(),
                                tee_type.clone(),
                                input,
                                timeout,
                                false,
                                allow_fallback,
                            ).await;
                            
                            (result, operation_id)
                        });
                        
                        chunk_tasks.push(task);
                    }
                    
                    // Collect results for this chunk
                    for task in chunk_tasks {
                        match task.await {
                            Ok((Ok(mut result), operation_id)) => {
                                result.operation_id = Some(operation_id);
                                chunk_results.push(result);
                            },
                            Ok((Err(e), operation_id)) => {
                                error!("Error executing operation on {}: {}", target_id_clone, e);
                                
                                // Mark connection as failed
                                if let Ok(tee_type) = tee_type_clone.to_string().parse::<TeeType>() {
                                    self_clone.connection_pool.mark_connection_failed(
                                        &target_id_clone, 
                                        &region_id_clone, 
                                        &tee_type
                                    );
                                }
                                
                                // Add error result
                                let result = MeshExecutionResult {
                                    result: Vec::new(),
                                    execution_time_ns: 0,
                                    network_latency_ns: 0,
                                    attestations: None,
                                    error: Some(format!("Execution error: {}", e)),
                                    metrics: None,
                                    state_hash: self_clone.get_random_hash(),
                                    memory_used: 0,
                                    syscall_count: 0,
                                    status: "error".to_string(),
                                    cache_hit: false,
                                    cache_ttl_sec: None,
                                    execution_type: "error".to_string(),
                                    operation_id: Some(operation_id),
                                };
                                chunk_results.push(result);
                            },
                            Err(e) => {
                                error!("Task error executing operation on {}: {}", target_id_clone, e);
                                
                                // Mark connection as failed
                                if let Ok(tee_type) = tee_type_clone.to_string().parse::<TeeType>() {
                                    self_clone.connection_pool.mark_connection_failed(
                                        &target_id_clone, 
                                        &region_id_clone, 
                                        &tee_type
                                    );
                                }
                            }
                        }
                    }
                    
                    chunk_results
                });
                
                handles.push(handle);
            }
        }
        
        // Collect all results
        for handle in handles {
            match handle.await {
                Ok(mut chunk_results) => {
                    results.append(&mut chunk_results);
                },
                Err(e) => {
                    error!("Error joining task: {}", e);
                }
            }
        }
        
        let total_duration = start_time.elapsed();
        
        // Calculate performance metrics
        let mut execution_times = Vec::new();
        let successful_ops = results.iter()
            .filter(|r| r.error.is_none())
            .count();
        
        for result in &results {
            if result.error.is_none() {
                execution_times.push(result.execution_time_ns as f64 / 1_000_000.0); // Convert to ms
            }
        }
        
        // Calculate percentiles if we have data
        let (p50, p95, p99, max_latency, min_latency, avg_latency) = if !execution_times.is_empty() {
            execution_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p50 = self.percentile(&execution_times, 50.0);
            let p95 = self.percentile(&execution_times, 95.0);
            let p99 = self.percentile(&execution_times, 99.0);
            let max = *execution_times.last().unwrap();
            let min = *execution_times.first().unwrap();
            let avg = execution_times.iter().sum::<f64>() / execution_times.len() as f64;
            (Some(p50), Some(p95), Some(p99), Some(max), Some(min), Some(avg))
        } else {
            (None, None, None, None, None, None)
        };
        
        // Calculate operations per second
        let ops_per_sec = if successful_ops > 0 {
            let duration_secs = total_duration.as_secs_f64();
            if duration_secs > 0.0 {
                Some((successful_ops as f64 / duration_secs).round())
            } else {
                None
            }
        } else {
            None
        };
        
        // Calculate network efficiency
        let network_efficiency = self.connection_pool.healthy_connection_count() as f64 / 
            self.connection_pool.connection_count().max(1) as f64;
        
        // Create batch result with performance metrics
        let performance = if successful_ops > 0 {
            Some(PerformanceMetrics {
                tee_type: "batch".to_string(),
                region_id: "multiple".to_string(),
                worker_id: "batch".to_string(),
                latency_ms: avg_latency.unwrap_or(0.0),
                execution_time_ns: total_duration.as_nanos() as u64,
                network_latency_ms: 0.0, // Combined in execution time
                success_count: successful_ops as u64,
                failure_count: (results.len() - successful_ops) as u64,
                memory_used_bytes: 0, // Unknown for batch
                syscall_count: 0, // Unknown for batch
                throughput_bytes_ps: 0, // Unknown for batch
                p50_execution_ms: p50.map(|v| v as u64),
                p95_execution_ms: p95.map(|v| v as u64),
                p99_execution_ms: p99.map(|v| v as u64),
                max_execution_ms: max_latency.map(|v| v as u64),
                avg_execution_ms: avg_latency,
                min_execution_ms: min_latency.map(|v| v as u64),
                operations_per_second: ops_per_sec.map(|v| v as u64),
                batch_size: Some(batch_size.try_into().unwrap()),
                concurrent_operations: Some(max_concurrent.try_into().unwrap()),
                network_efficiency: Some(network_efficiency),
            })
        } else {
            None
        };
        
        // Periodically clean up idle connections (do this after a certain number of batches)
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < 5 { // 5% chance
            let removed = self.connection_pool.cleanup_idle_connections();
            if removed > 0 {
                debug!("Cleaned up {} idle connections", removed);
            }
        }
        
        Ok(BatchExecutionResult {
            batch_id,
            operations: results,
            total_execution_time_ms: (total_duration.as_nanos() / 1_000_000) as u64,
            batch_attestation: None, // We're not adding attestation in test mode
            performance,
        })
    }
    
    // Helper function to calculate percentiles
    fn percentile(&self, values: &Vec<f64>, percentile: f64) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        
        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let index = (percentile / 100.0 * (sorted_values.len() - 1) as f64) as usize;
        sorted_values[index]
    }

    // Helper function to generate a random hash for testing
    fn get_random_hash(&self) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let mut hash = Vec::with_capacity(32);
        for _ in 0..32 {
            hash.push(rng.gen::<u8>());
        }
        hash
    }

    // Clone this struct for usage in tokio spawn
    pub fn clone(&self) -> Self {
        let peers_clone = match self.peers.read() {
            Ok(peers) => {
                let mut new_peers = HashMap::new();
                for (key, value) in peers.iter() {
                    new_peers.insert(key.clone(), value.clone());
                }
                new_peers
            },
            Err(_) => HashMap::new(),
        };
        
        let cache_clone = match self.cache.read() {
            Ok(cache) => {
                let mut new_cache = HashMap::new();
                for (key, value) in cache.iter() {
                    new_cache.insert(key.clone(), value.clone());
                }
                new_cache
            },
            Err(_) => HashMap::new(),
        };
        
        let local_state_clone = match self.local_state.read() {
            Ok(state) => {
                let mut new_state = HashMap::new();
                for (key, value) in state.iter() {
                    new_state.insert(key.clone(), value.clone());
                }
                new_state
            },
            Err(_) => HashMap::new(),
        };
        
        MeshCoordinator {
            config: self.config.clone(),
            peers: RwLock::new(peers_clone),
            cache: RwLock::new(cache_clone),
            local_state: RwLock::new(local_state_clone),
            // Create a new semaphore with the same permits as the original
            max_concurrent_executions: Semaphore::new(self.config.max_peers),
            connection_pool: self.connection_pool.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub enclave_type: String,
    pub measurement: Vec<u8>,
    pub timestamp: u64,
    pub platform_data: Vec<u8>,
}
