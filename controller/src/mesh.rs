// Module for implementing mesh network functionality in the TEE controller
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::time;
use log::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use rand::Rng;
use tokio::sync::Semaphore;
use uuid::Uuid;
use reqwest::{Client as HttpClient, ClientBuilder};
use crate::discovery_service::{LocalityInfoDto, DiscoveryRegionInfo};
use crate::accumulator_client::{AccumulatorClientTrait, RealAccumulatorClient, MockAccumulatorClient};
use crate::accumulator_client::{create_accumulator_client};

// Define TeeType enum for mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeeType {
    /// Intel SGX
    IntelSGX,
    /// AMD SEV
    SEV,
}

impl ToString for TeeType {
    fn to_string(&self) -> String {
        match self {
            TeeType::IntelSGX => "IntelSGX".to_string(),
            TeeType::SEV => "SEV".to_string(),
        }
    }
}

// Implement FromStr for TeeType for string parsing
impl std::str::FromStr for TeeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "INTEL_SGX" => Ok(TeeType::IntelSGX),
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
    // Add configuration for enhanced discovery service
    pub enhanced_discovery: bool,
    pub discovery_config: Option<DiscoveryServiceConfig>,
    // Add new fields for accumulator configuration
    pub accumulator_endpoint: Option<String>,
    pub local_identity: Option<String>,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            region_id: "us-west".to_string(),
            endpoint: "https://example.com".to_string(),
            tee_id: "tee-123".to_string(),
            max_peers: 10,
            discovery_interval_sec: 60,
            discovery_endpoint: "https://discovery.example.com".to_string(),
            circuit_breaker_threshold: Duration::from_secs(30),
            peer_refresh_interval: Duration::from_secs(300),
            enhanced_discovery: false,
            discovery_config: None,
            // Default values for new accumulator fields
            accumulator_endpoint: None,
            local_identity: None,
        }
    }
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
    last_updated: std::time::SystemTime,
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
    // Flag to determine if we use simulation mode or real connections
    simulation_mode: bool,
    // HTTP client for real connections
    http_client: Option<HttpClient>,
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
            simulation_mode: true, // Default to simulation mode for backward compatibility
            http_client: None,
        }
    }
    
    // New constructor for creating a connection pool with real connections
    pub fn new_with_real_connections(max_connections: usize, 
                                  connection_timeout: Duration,
                                  idle_timeout: Duration) -> Self {
        // Create an HTTP client with appropriate timeouts and connection pooling
        let http_client = ClientBuilder::new()
            .timeout(connection_timeout)
            .pool_idle_timeout(idle_timeout)
            .pool_max_idle_per_host(max_connections)
            .build()
            .ok();
            
        ConnectionPool {
            connections: RwLock::new(HashMap::new()),
            max_connections,
            connection_timeout,
            idle_timeout,
            max_failures: 3,
            connection_semaphore: Arc::new(Semaphore::new(10)),
            simulation_mode: false, // Use real connections
            http_client,
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
        // Generate connection key
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        
        // Try to acquire semaphore permit (non-blocking)
        let _permit = match self.connection_semaphore.try_acquire() {
            Ok(permit) => Some(permit),
            Err(_) => {
                warn!("Failed to acquire connection semaphore, connection creation may be throttled");
                None
            }
        };
        
        // Check if connection already exists
        let mut connections = self.connections.write().unwrap();
        if let Some(existing_conn) = connections.get(&key) {
            // Return existing connection if it exists
            return existing_conn.clone();
        }
        
        // Create new connection info
        let mut conn_info = ConnectionInfo {
            peer_id: peer_id.to_string(),
            region_id: region_id.to_string(),
            tee_type: *tee_type,
            endpoint: endpoint.to_string(),
            created_at: std::time::Instant::now(),
            last_used: std::time::Instant::now(),
            use_count: 0,
            failed_attempts: 0,
            is_healthy: true,
            latency_ms: 0.0,
        };
        
        if !self.simulation_mode {
            // For real connections, attempt to establish an actual TCP connection
            if let Some(client) = &self.http_client {
                // Since this is just a connection health check, we'll use a simple
                // asynchronous approach with a timeout. In a real implementation,
                // this would be better integrated with the tokio runtime.
                let endpoint_str = endpoint.to_string();
                
                // Simulate connection latency based on random network conditions
                // This avoids actual network requests during development but still
                // provides realistic behavior for testing real connection pooling
                let mut rng = rand::thread_rng();
                let success_rate = 0.95; // 95% success rate for initial connections
                let is_successful = rng.gen::<f64>() < success_rate;
                
                // Add realistic network latency simulation
                let base_latency = 20.0; // base latency in ms
                let jitter = rng.gen::<f64>() * 15.0; // up to 15ms of jitter
                let simulated_latency = base_latency + jitter;
                
                // Set connection properties based on simulated connection
                conn_info.latency_ms = simulated_latency;
                conn_info.is_healthy = is_successful;
                
                if !conn_info.is_healthy {
                    conn_info.failed_attempts = 1;
                    warn!("Failed to establish connection to {}", endpoint_str);
                } else {
                    debug!("Established connection to {} with latency {}ms", endpoint_str, simulated_latency);
                }
            } else {
                warn!("HTTP client not initialized, falling back to simulated connection");
                // Fall back to simulation behavior
                conn_info.latency_ms = 10.0;
            }
        } else {
            // In simulation mode, initialize with default values
            conn_info.latency_ms = 10.0;
        }
        
        // Store connection in pool
        connections.insert(key, conn_info.clone());
        
        conn_info
    }
    
    // Mark a connection as failed
    pub fn mark_connection_failed(&self, peer_id: &str, region_id: &str, tee_type: &TeeType) {
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        let mut connections = self.connections.write().unwrap();
        
        if let Some(conn) = connections.get_mut(&key) {
            conn.failed_attempts += 1;
            if conn.failed_attempts >= self.max_failures {
                conn.is_healthy = false;
                warn!("Connection to {}:{}/{} marked as unhealthy after {} failures", 
                      region_id, tee_type.to_string(), peer_id, conn.failed_attempts);
            }
        }
    }
    
    // Mark a connection as healthy after successful use
    pub fn mark_connection_healthy(&self, peer_id: &str, region_id: &str, tee_type: &TeeType, latency_ms: f64) {
        let key = format!("{}:{}:{}", region_id, tee_type.to_string(), peer_id);
        let mut connections = self.connections.write().unwrap();
        
        if let Some(conn) = connections.get_mut(&key) {
            conn.is_healthy = true;
            conn.failed_attempts = 0;
            conn.last_used = std::time::Instant::now();
            conn.use_count += 1;
            
            // Update latency with exponential moving average (more weight to recent measurements)
            if conn.latency_ms > 0.0 {
                // 70% weight to new measurement, 30% to history
                conn.latency_ms = (latency_ms * 0.7) + (conn.latency_ms * 0.3);
            } else {
                conn.latency_ms = latency_ms;
            }
        }
    }
    
    // Clean up idle connections
    pub fn cleanup_idle_connections(&self) -> usize {
        let now = std::time::Instant::now();
        let mut connections = self.connections.write().unwrap();
        
        // Identify connections to remove (collect keys to avoid borrowing issues)
        let idle_keys: Vec<String> = connections.iter()
            .filter(|(_, conn)| now.duration_since(conn.last_used) > self.idle_timeout)
            .map(|(key, _)| key.clone())
            .collect();
        
        let count = idle_keys.len();
        
        // Remove idle connections from the pool
        for key in idle_keys {
            connections.remove(&key);
            debug!("Removed idle connection: {}", key);
        }
        
        // If we're still over max connections, remove the least recently used healthy connections
        if connections.len() > self.max_connections {
            let mut keys_by_usage: Vec<_> = connections.iter()
                .map(|(k, v)| (k.clone(), v.last_used))
                .collect();
            
            // Sort by last used time (oldest first)
            keys_by_usage.sort_by(|a, b| a.1.cmp(&b.1));
            
            // Take just enough to get below max_connections
            let to_remove = connections.len() - self.max_connections;
            let remove_keys: Vec<_> = keys_by_usage.iter()
                .take(to_remove)
                .map(|(k, _)| k.clone())
                .collect();
                
            for key in remove_keys {
                connections.remove(&key);
                debug!("Removed least recently used connection: {}", key);
            }
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
            simulation_mode: self.simulation_mode,
            http_client: self.http_client.clone(),
        }
    }
}

// Mesh coordinator
pub struct MeshCoordinator {
    pub config: MeshConfig,
    peers: Arc<RwLock<HashMap<String, PeerState>>>,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    local_state: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    max_concurrent_executions: Semaphore,
    connection_pool: Arc<ConnectionPool>,
    // Add accumulator client reference
    accumulator_client: Option<Arc<dyn AccumulatorClientTrait>>,
}

impl MeshCoordinator {
    pub async fn new(config: MeshConfig) -> Result<Arc<Self>, std::io::Error> {
        let max_peers = config.max_peers;
        
        // Create a connection pool for peer communication
        let connection_pool = {
            debug!("Creating connection pool for up to {} peers", max_peers * 2);
            Arc::new(ConnectionPool::new(max_peers * 2)) // Support up to 2x max_peers connections
        };
        
        // Create accumulator client
        let accumulator_client = if config.enhanced_discovery {
            let endpoint = config.discovery_config.as_ref()
                .and_then(|c| c.accumulator_endpoint.clone())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
                
            let local_identity = config.local_identity.clone()
                .unwrap_or_else(|| "unknown".to_string());
            
            // Create a real accumulator client
            Some(Arc::new(RealAccumulatorClient::new(&endpoint, &local_identity)) as Arc<dyn AccumulatorClientTrait>)
        } else {
            // Create a mock accumulator client for backward compatibility
            Some(Arc::new(MockAccumulatorClient::new()) as Arc<dyn AccumulatorClientTrait>)
        };
        
        let coordinator = Arc::new(MeshCoordinator {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            local_state: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent_executions: Semaphore::new(max_peers),
            connection_pool,
            accumulator_client,
        });
        
        // Initialize and start peer discovery
        // Clone the Arc to avoid borrowing issues in the discovery task
        let self_clone = Arc::clone(&coordinator);
        tokio::spawn(async move {
            // Wait a bit before starting discovery to allow other initialization to complete
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            // Run the initial peer discovery
            match self_clone.refresh_peers().await {
                Ok(_) => debug!("Initial peer discovery completed"),
                Err(e) => error!("Failed to run initial peer discovery: {}", e),
            }
            
            // Set up recurring peer discovery
            let mut interval = tokio::time::interval(Duration::from_secs(self_clone.config.discovery_interval_sec));
            
            loop {
                interval.tick().await;
                
                match self_clone.refresh_peers().await {
                    Ok(_) => debug!("Periodic peer discovery completed"),
                    Err(e) => error!("Failed to run periodic peer discovery: {}", e),
                }
            }
        });
        
        Ok(coordinator)
    }
    
    async fn start_discovery(self: Arc<Self>) -> Result<(), std::io::Error> {
        info!("Initializing mesh coordinator for TEE ID: {}", self.config.tee_id);
        
        // Start periodic peer discovery
        let config_clone = self.config.clone();
        let coordinator = self.clone();
        
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
        
        // Check if we have an accumulator client
        if let Some(accumulator_client) = &self.accumulator_client {
            // Retrieve peers from the accumulator client
            match accumulator_client.get_peers_in_region(&self.config.region_id).await {
                Ok(peer_ids) => {
                    debug!("Found {} peers in region {}", peer_ids.len(), self.config.region_id);
                    
                    // Batch verify the peers using the accumulator
                    let verification_results = match accumulator_client.batch_verify_peers(&peer_ids).await {
                        Ok(results) => results,
                        Err(e) => {
                            error!("Failed to verify peers: {}", e);
                            // Create a vector of "false" results with the same length
                            vec![false; peer_ids.len()]
                        }
                    };
                    
                    // Process each peer
                    let mut peers = self.peers.write().unwrap();
                    
                    // Track discovered peer types to update our local state
                    let mut discovered_types: HashSet<String> = HashSet::new();
                    
                    for (i, peer_id) in peer_ids.iter().enumerate() {
                        let is_valid = verification_results.get(i).copied().unwrap_or(false);
                        
                        if is_valid {
                            // Peer is valid, get more information about it
                            // For now, we'll extract tee_type from the peer_id format "id:type"
                            let tee_type_str = peer_id.split(':').nth(1).unwrap_or("IntelSGX");
                            let tee_type = match tee_type_str.parse::<TeeType>() {
                                Ok(t) => t,
                                Err(_) => {
                                    warn!("Invalid TEE type in peer ID: {}", peer_id);
                                    TeeType::IntelSGX  // Default to IntelSGX
                                }
                            };
                            
                            // Create a key for this peer type in the region
                            let peer_key = format!("{}:{}", self.config.region_id, tee_type.to_string());
                            discovered_types.insert(peer_key.clone());
                            
                            // Generate a simulated endpoint for this peer (would be real in production)
                            let endpoint = if self.config.enhanced_discovery {
                                // In production, we would get real endpoints from a service directory
                                format!("https://{}.{}.example.com", peer_id, self.config.region_id) 
                            } else {
                                // Simulated endpoint for testing
                                format!("http://localhost:808{}", i % 10)
                            };
                            
                            // Update or add the peer
                            if let Some(existing_peer) = peers.get_mut(&peer_key) {
                                // Update existing peer
                                existing_peer.tee_id = peer_id.clone();
                                existing_peer.tee_type = tee_type.to_string();
                                existing_peer.region_id = self.config.region_id.clone();
                                existing_peer.endpoint = endpoint;
                                existing_peer.status = "active".to_string();
                                existing_peer.last_updated = std::time::SystemTime::now();
                                existing_peer.latency_history = Vec::new();
                                existing_peer.success_count = 0;
                                existing_peer.failure_count = 0;
                            } else {
                                // Add new peer
                                let peer_state = PeerState {
                                    tee_id: peer_id.clone(),
                                    tee_type: tee_type.to_string(),
                                    region_id: self.config.region_id.clone(),
                                    endpoint,
                                    status: "active".to_string(),
                                    last_updated: std::time::SystemTime::now(),
                                    latency_history: Vec::new(),
                                    success_count: 0,
                                    failure_count: 0,
                                };
                                peers.insert(peer_key.clone(), peer_state);
                            }
                        } else {
                            warn!("Peer {} failed verification", peer_id);
                        }
                    }
                    
                    // Remove peers that were not in this discovery round
                    // We only remove peers for types we discovered this round to avoid removing peers of other types
                    peers.retain(|key, peer| {
                        let key_parts: Vec<&str> = key.split(':').collect();
                        if key_parts.len() == 2 && key_parts[0] == self.config.region_id && discovered_types.contains(key) {
                            // This is a peer in our region of a type we discovered this round
                            // Keep it only if it's recently updated
                            if let Ok(duration) = std::time::SystemTime::now().duration_since(peer.last_updated) {
                                // Keep peers seen in the last hour
                                duration < Duration::from_secs(3600)
                            } else {
                                // If time went backwards, keep the peer
                                true
                            }
                        } else {
                            // This is a peer from another region or of a type we didn't discover this round
                            // Keep it
                            true
                        }
                    });
                    
                    debug!("After refresh: {} peers in region {}", 
                           peers.values().filter(|p| p.region_id == self.config.region_id).count(),
                           self.config.region_id);
                }
                Err(e) => {
                    error!("Failed to get peers from accumulator: {}", e);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to get peers: {}", e)
                    ));
                }
            }
        } else {
            // No accumulator client, use simulated peers for backward compatibility
            debug!("No accumulator client available, using simulated peers");
            
            // Create simulated peers for testing
            let mut peers = self.peers.write().unwrap();
            
            // Add a simulated peer for each TEE type in the region
            for tee_type in [TeeType::IntelSGX, TeeType::SEV].iter() {
                let peer_key = format!("{}:{}", self.config.region_id, tee_type.to_string());
                let peer_id = format!("sim-{}-{}", self.config.region_id, tee_type.to_string());
                
                // Create simulated endpoint
                let endpoint = format!("http://localhost:8080");
                
                // Add or update peer
                let peer_state = PeerState {
                    tee_id: peer_id.clone(),
                    tee_type: tee_type.to_string(),
                    region_id: self.config.region_id.clone(),
                    endpoint,
                    status: "active".to_string(),
                    last_updated: std::time::SystemTime::now(),
                    latency_history: Vec::new(),
                    success_count: 0,
                    failure_count: 0,
                };
                
                peers.insert(peer_key.clone(), peer_state);
                info!("Added simulated peer: {} in region {}", peer_id, self.config.region_id);
            }
        }
        
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
        use sha2::{Sha256, Digest};
        
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
                latency_ms: p.latency_history.last().unwrap_or(&Duration::from_millis(0)).as_millis() as f64,
            })
            .collect();
        
        // If no peers are found, create simulated peers for testing
        if filtered_peers.is_empty() {
            let mut simulated_peers = Vec::new();
            for i in 0..3.min(max_results) {
                let tee_type_value = if let Some(t) = tee_type.clone() {
                    t
                } else if i % 2 == 0 {
                    "IntelSGX".to_string()
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
        self: Arc<Self>,  // Change &self to self: Arc<Self> to ensure ownership
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
}

// Clone implementation for MeshCoordinator
impl Clone for MeshCoordinator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            peers: Arc::new(RwLock::new(self.peers.read().unwrap().clone())),
            cache: Arc::new(RwLock::new(self.cache.read().unwrap().clone())),
            local_state: Arc::new(RwLock::new(self.local_state.read().unwrap().clone())),
            // Create a new semaphore with the same permits as the original
            max_concurrent_executions: Semaphore::new(self.config.max_peers),
            connection_pool: self.connection_pool.clone(),
            accumulator_client: self.accumulator_client.clone(),
        }
    }
}

// Attestation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub enclave_type: String,
    pub measurement: Vec<u8>,
    pub timestamp: u64,
    pub platform_data: Vec<u8>,
}

// DiscoveryServiceConfig is a placeholder for the enhanced discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryServiceConfig {
    /// List of initial bootstrap peers
    pub bootstrap_peers: Vec<String>,
    
    /// Local peer's ID
    pub peer_id: String,
    
    /// Region ID for the peer
    pub region_id: String,
    
    /// Locality information for the peer
    pub locality: Option<LocalityInfoDto>,
    
    /// Maximum connections per region
    pub max_connections_per_region: usize,
    
    /// Maximum inactive time for connections before pruning
    pub max_inactive_time_sec: u64,
    
    /// Time interval (in seconds) for heartbeat messages
    pub heartbeat_interval_sec: u64,
    
    /// Maximum number of peers to exchange during discovery
    pub max_peers_exchange: usize,
    
    /// Maximum age of peer information before it's considered stale
    pub max_peer_age_sec: u64,
    
    /// Maximum number of peers to maintain in super peer set
    pub max_superpeers: usize,
    
    /// Whether to enable peer gossip
    pub enable_gossip: bool,
    
    /// Maximum number of hops for peer gossip
    pub max_gossip_hops: u32,
    
    /// Whether to use enhanced discovery (with accumulator)
    pub enhanced_discovery: bool,
    
    /// Local identity for attestation (base64 encoded)
    pub local_identity: Option<String>,
    
    /// Endpoint for the accumulator service
    pub accumulator_endpoint: Option<String>,
}

impl MeshCoordinator {
    pub fn compute_mesh_state_hash(&self) -> Vec<u8> {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        
        // Add peers to hash
        if let Ok(peers) = self.peers.read() {
            for (key, peer) in peers.iter() {
                hasher.update(key.as_bytes());
                hasher.update(peer.tee_id.as_bytes());
                hasher.update(peer.endpoint.as_bytes());
            }
        }
        
        // Add local state to hash
        if let Ok(state) = self.local_state.read() {
            for (key, value) in state.iter() {
                hasher.update(key.as_bytes());
                hasher.update(value);
            }
        }
        
        let hash = hasher.finalize().to_vec();
        hash
    }
}
