use crate::mesh::MeshCoordinator;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::{Arc, RwLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time;
use tokio::task::JoinHandle;
use log::{debug, info, warn, error};
use tracing::info as tracing_info;
use serde::{Serialize, Deserialize};
use tee_interface::{TeeError, types::RegionInfo as TeeRegionInfo};
use tee_interface::types::TeeAttestation as AttestationReport;

/// Get the current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

// Mock structures that would normally come from the accumulator crate
pub struct DiscoveryParams {
    pub max_batch_size: u64,
    pub verification_threshold: u64,
}

/// Information about a region in the discovery service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRegionInfo {
    /// Region identifier
    pub region_id: String,
    /// Number of executors in the region
    pub executor_count: usize,
    /// Whether this is a leaf region (no child regions)
    pub is_leaf: bool,
    /// Parent region (if any)
    pub parent_region: Option<String>,
}

// Mock result for batch attestation operations
pub struct BatchAttestationResult {
    pub success_count: u64,
    pub failed_count: u64,
}

// Gossip protocol message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    Ping,
    Pong {
        executor_id: String,
    },
    RegionUpdate {
        region_id: String,
        executor_count: u64,
        super_peers: Vec<String>,
        last_update: u64,
    },
    PeerAnnounce {
        region_id: String,
        executor_id: String,
        locality: Option<LocalityInfoDto>,
    },
    RoutingTable {
        proximity_updates: Vec<(String, String, u32)>, // region1, region2, distance
    },
    RegionHierarchy {
        parent: String,
        child: String,
    },
    RegionProximityUpdate {
        from_region: String,
        to_region: String,
        proximity: u32,
    },
    SuperPeerAnnounce {
        region_id: String,
        executor_id: String,
    },
    SuperPeerRemove {
        region_id: String,
        executor_id: String,
    },
}

/// Network coordinates for locality awareness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkCoordinates {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
    /// Z coordinate (optional)
    pub z: Option<f64>,
}

/// Latency profile for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyProfile {
    /// Average latency in milliseconds
    pub avg_latency_ms: u32,
    /// Standard deviation of latency
    pub std_dev_ms: u32,
    /// Maximum observed latency
    pub max_latency_ms: u32,
    /// Minimum observed latency
    pub min_latency_ms: u32,
}

/// Detailed information about locality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalityInfo {
    /// Region ID where the node is located
    pub region_id: String,
    /// Zone ID within the region (optional)
    pub zone_id: Option<String>,
    /// Network coordinates for spatial positioning
    pub coordinates: Option<NetworkCoordinates>,
    /// Latency profile based on measurements
    pub latency_profile: Option<LatencyProfile>,
    /// Last update timestamp
    pub last_update: u64,
}

/// Data transfer object for locality information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalityInfoDto {
    /// Region ID where the node is located
    pub region_id: String,
    /// Zone ID within the region (optional)
    pub zone_id: Option<String>,
    /// Network coordinates for spatial positioning (x, y, z)
    pub coordinates: Option<NetworkCoordinatesDto>,
    /// Latency profile data
    pub latency_profile: Option<LatencyProfileDto>,
    /// Last update timestamp
    pub last_update: u64,
}

/// Data transfer object for network coordinates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkCoordinatesDto {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
    /// Z coordinate (optional)
    pub z: Option<f64>,
}

/// Data transfer object for latency profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyProfileDto {
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Standard deviation of latency
    pub std_dev_ms: f64,
    /// Maximum observed latency
    pub max_latency_ms: f64,
    /// Minimum observed latency
    pub min_latency_ms: f64,
}

impl From<LocalityInfoDto> for LocalityInfo {
    fn from(dto: LocalityInfoDto) -> Self {
        LocalityInfo {
            region_id: dto.region_id,
            zone_id: dto.zone_id,
            latency_profile: dto.latency_profile.map(|profile| LatencyProfile {
                avg_latency_ms: profile.avg_latency_ms as u32,
                std_dev_ms: profile.std_dev_ms as u32,
                max_latency_ms: profile.max_latency_ms as u32,
                min_latency_ms: profile.min_latency_ms as u32,
            }),
            coordinates: dto.coordinates.map(|coords| NetworkCoordinates {
                x: coords.x,
                y: coords.y,
                z: coords.z,
            }),
            last_update: dto.last_update,
        }
    }
}

/// Discovery message types for peer communications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryMessage {
    /// Hello message to establish initial connection
    Hello {
        /// Peer ID of the sender
        peer_id: String,
        /// Region ID of the sender
        region_id: String,
        /// Locality information for the sender
        locality: Option<LocalityInfoDto>,
        /// Timestamp of the message
        timestamp: u64,
    },
    /// Response to Hello message
    Welcome {
        /// Peer ID of the sender
        peer_id: String,
        /// List of known peers in the network
        known_peers: HashMap<String, PeerInfo>,
        /// Timestamp of the message
        timestamp: u64,
    },
    /// Heartbeat to maintain connection liveness
    Heartbeat {
        /// Peer ID of the sender
        peer_id: String,
        /// Timestamp of the message
        timestamp: u64,
    },
    /// Peer status update notification
    PeerUpdate {
        /// Peer ID of the sender
        peer_id: String,
        /// List of updated peer information
        updated_peers: HashMap<String, PeerInfo>,
        /// Timestamp of the message
        timestamp: u64,
    },
    /// Request for peer information
    PeerQuery {
        /// Peer ID of the sender
        peer_id: String,
        /// Optional region filter
        region_id: Option<String>,
        /// Timestamp of the message
        timestamp: u64,
    },
}

/// Peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub peer_id: String,
    /// Region ID
    pub region_id: String,
    /// Locality information
    pub locality: Option<LocalityInfoDto>,
    /// Timestamp of last update
    pub last_update: u64,
}

/// Enhanced discovery service that leverages the accumulator for efficient peer management
pub struct DiscoveryService {
    /// Core mesh coordinator for execution routing
    mesh: Arc<MeshCoordinator>,
    
    /// Client for the accumulator service
    accumulator_client: Arc<AccumulatorClient>,
    
    /// Cache of recently verified peers
    peer_cache: Arc<RwLock<HashMap<String, Instant>>>,
    
    /// TTL for the peer cache in seconds
    cache_ttl: u64,
    
    /// Maximum batch size for attestation batching
    max_batch_size: u64,
    
    /// Mapping of region names to super peers
    super_peers: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    
    /// Region hierarchies - mapping child regions to parent regions
    region_hierarchy: Arc<RwLock<HashMap<String, String>>>,
    
    /// Proximity map for locality-aware routing
    proximity_map: Arc<RwLock<HashMap<String, HashMap<String, u32>>>>,
    
    /// Locality information for regions and executors
    locality_info: Arc<RwLock<HashMap<String, LocalityInfo>>>,
    
    /// Gossip protocol state
    gossip_peers: Arc<RwLock<HashMap<String, Instant>>>, // last gossip time
    
    /// Gossip message queue
    gossip_queue: Arc<Mutex<VecDeque<(String, GossipMessage)>>>,
    
    /// Gossip interval in seconds
    gossip_interval: u64,
    
    /// Flag to control gossip service
    gossip_enabled: Arc<RwLock<bool>>,
    
    /// Background task handle for gossip protocol
    gossip_task: Option<JoinHandle<()>>,
    
    /// Channel for communicating with gossip task
    gossip_tx: Option<mpsc::Sender<()>>,
    
    /// Region cache that stores executors by region
    region_cache: RwLock<HashMap<String, RegionCache>>,
    
    /// Service configuration parameters
    params: DiscoveryServiceConfig,
    
    /// Last refresh timestamp
    last_refresh: RwLock<u64>,
    
    /// Region children map
    region_children: RwLock<HashMap<String, Vec<String>>>,
    
    /// Last seen timestamps for super peers
    super_peers_last_seen: RwLock<HashMap<String, Instant>>,
    
    /// Connection statistics for peers
    connection_stats: Arc<RwLock<HashMap<String, ConnectionStats>>>,
}

/// Cache structure for a region
#[derive(Clone, Default)]
struct RegionCache {
    /// Executors in this region
    executors: HashSet<String>,
    
    /// Last updated timestamp
    last_update: u64,
    
    /// Accumulator value for this region
    accumulator_value: Vec<u8>,
}

impl RegionCache {
    fn new() -> Self {
        RegionCache {
            executors: HashSet::new(),
            last_update: current_timestamp(),
            accumulator_value: vec![0u8; 32],
        }
    }
    
    fn add_executor(&mut self, executor_id: String) {
        self.executors.insert(executor_id);
        self.last_update = current_timestamp();
    }
}

/// Configuration for the discovery service
#[derive(Debug, Clone)]
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
}

impl Default for DiscoveryServiceConfig {
    fn default() -> Self {
        DiscoveryServiceConfig {
            bootstrap_peers: Vec::new(),
            peer_id: "local-peer".to_string(),
            region_id: "default-region".to_string(),
            locality: None,
            max_connections_per_region: 10,
            max_inactive_time_sec: 300,
            heartbeat_interval_sec: 30,
            max_peers_exchange: 20,
            max_peer_age_sec: 300,
            max_superpeers: 5,
            enable_gossip: true,
            max_gossip_hops: 2,
        }
    }
}

/// Result of batch registration operation
pub struct BatchRegistrationResult {
    /// Number of successfully registered peers
    pub success_count: u64,
    
    /// Number of peers that failed to register
    pub failed_count: u64,
    
    /// IDs of all peers
    pub peers: Vec<String>,
}

#[derive(Debug, Clone)]
struct ConnectionStats {
    /// Last time this connection was used
    last_used: Instant,
    /// Number of successful messages
    successful_messages: u64,
    /// Number of failed messages
    failed_messages: u64,
    /// Average response time in milliseconds
    avg_response_time_ms: f64,
    /// Locality information if available
    locality: Option<LocalityInfoDto>,
}

impl ConnectionStats {
    /// Create new connection stats
    fn new(locality: Option<LocalityInfoDto>) -> Self {
        Self {
            last_used: Instant::now(),
            successful_messages: 0,
            failed_messages: 0,
            avg_response_time_ms: 0.0,
            locality,
        }
    }
    
    /// Update statistics after a successful message
    fn update_success(&mut self, response_time_ms: f64) {
        self.last_used = Instant::now();
        self.successful_messages += 1;
        
        // Update average response time
        let total_messages = self.successful_messages + self.failed_messages;
        let old_weight = (total_messages - 1) as f64 / total_messages as f64;
        let new_weight = 1.0 / total_messages as f64;
        
        self.avg_response_time_ms = self.avg_response_time_ms * old_weight + response_time_ms * new_weight;
    }
    
    /// Update statistics after a failed message
    fn update_failure(&mut self) {
        self.last_used = Instant::now();
        self.failed_messages += 1;
    }
    
    /// Calculate reliability score (0.0-1.0)
    fn reliability_score(&self) -> f64 {
        let total = self.successful_messages + self.failed_messages;
        if total == 0 {
            return 0.5; // Default for new connections
        }
        self.successful_messages as f64 / total as f64
    }
}

// Mock AccumulatorClient for discovery service integration
// This will be replaced with actual implementation later
#[derive(Debug, Clone)]
pub struct AccumulatorClient {
    // Mock fields
    pub endpoint: String,
}

impl AccumulatorClient {
    pub fn new() -> Self {
        AccumulatorClient {
            endpoint: "http://localhost:8080".to_string(),
        }
    }
    
    pub async fn register_peer(&self, _peer_id: &str, _region_id: &str) -> Result<(), String> {
        // Mock implementation
        Ok(())
    }
    
    pub async fn get_peers_in_region(&self, _region_id: &str) -> Result<Vec<String>, String> {
        // Mock implementation
        Ok(Vec::new())
    }
    
    pub async fn get_all_regions(&self) -> Result<Vec<DiscoveryRegionInfo>, String> {
        // Mock implementation returning sample data
        let regions = vec![
            DiscoveryRegionInfo {
                region_id: "us-west".to_string(),
                executor_count: 5,
                is_leaf: true,
                parent_region: Some("us".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "us-east".to_string(),
                executor_count: 7,
                is_leaf: true,
                parent_region: Some("us".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "us".to_string(),
                executor_count: 12,
                is_leaf: false,
                parent_region: Some("global".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "eu".to_string(),
                executor_count: 8,
                is_leaf: false,
                parent_region: Some("global".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "global".to_string(),
                executor_count: 20,
                is_leaf: false,
                parent_region: None,
            },
        ];
        
        Ok(regions)
    }
    
    pub async fn verify_peer(&self, _peer_id: &str) -> Result<bool, String> {
        // Mock implementation 
        Ok(true)
    }
    
    pub async fn set_peer_locality(&self, _peer_id: &str, _locality: &LocalityInfoDto) -> Result<(), String> {
        // Mock implementation
        Ok(())
    }
    
    pub async fn batch_verify_peers(&self, peers: &[String]) -> Result<Vec<bool>, String> {
        // Mock implementation
        let mut results = Vec::with_capacity(peers.len());
        for _ in peers {
            results.push(true);
        }
        Ok(results)
    }
    
    pub async fn set_region_hierarchy(&self, _parent: &str, _child: &str) -> Result<(), String> {
        // Mock implementation
        Ok(())
    }
    
    pub async fn get_peers_by_proximity(&self, _region_id: &str) -> Result<Vec<String>, String> {
        // Mock implementation
        Ok(Vec::new())
    }
    
    pub async fn get_super_peers(&self, _region_id: &str) -> Result<Vec<String>, String> {
        // Mock implementation
        Ok(Vec::new())
    }
}

impl DiscoveryService {
    /// Create a new discovery service with default parameters
    pub async fn new(mesh: Arc<MeshCoordinator>) -> Result<Arc<Self>, TeeError> {
        Self::new_with_params(mesh, DiscoveryServiceConfig::default()).await
    }
    
    /// Create a new discovery service with custom parameters
    pub async fn new_with_params(
        mesh: Arc<MeshCoordinator>,
        params: DiscoveryServiceConfig,
    ) -> Result<Arc<Self>, TeeError> {
        tracing_info!("Initializing discovery service with custom parameters");
        
        // In a real implementation, we would initialize the accumulator contract
        // with parameters for the discovery service
        
        let service = Arc::new(Self {
            mesh,
            accumulator_client: Arc::new(AccumulatorClient::new()),
            peer_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: 3600, // 1 hour
            max_batch_size: 100,
            super_peers: Arc::new(RwLock::new(HashMap::new())),
            region_hierarchy: Arc::new(RwLock::new(HashMap::new())),
            proximity_map: Arc::new(RwLock::new(HashMap::new())),
            locality_info: Arc::new(RwLock::new(HashMap::new())),
            gossip_peers: Arc::new(RwLock::new(HashMap::new())),
            gossip_queue: Arc::new(Mutex::new(VecDeque::new())),
            gossip_interval: params.heartbeat_interval_sec,
            gossip_enabled: Arc::new(RwLock::new(params.enable_gossip)),
            gossip_task: None,
            gossip_tx: None,
            region_cache: RwLock::new(HashMap::new()),
            params,
            last_refresh: RwLock::new(0),
            region_children: RwLock::new(HashMap::new()),
            super_peers_last_seen: RwLock::new(HashMap::new()),
            connection_stats: Arc::new(RwLock::new(HashMap::new())),
        });
        
        // Start background refresh
        let service_clone = service.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    service_clone.params.heartbeat_interval_sec
                )).await;
                
                if let Err(e) = service_clone.refresh_cache().await {
                    warn!("Failed to refresh discovery cache: {:?}", e);
                }
            }
        });
        
        // Start gossip service if enabled
        if *match service.gossip_enabled.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        } {
            service.clone().start_gossip_background_task().await?;
        }
        
        Ok(service)
    }
    
    /// Register a peer with attestation information
    pub async fn register_peer(
        &self,
        peer_id: String,
        region_id: String,
        attestation: AttestationReport,
    ) -> Result<String, TeeError> {
        // In a real implementation, this would call register_with_region on the accumulator contract
        // which requires a Context from the contract execution environment
        // For now, we'll update the local cache to simulate the process
        
        let mut cache = match self.region_cache.write() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire write lock on region_cache: {}", e);
                return Err(TeeError::ExecutionError(format!("Failed to acquire lock: {}", e)));
            }
        };
        let region_cache = cache.entry(region_id.clone()).or_insert_with(|| RegionCache {
            executors: HashSet::new(),
            last_update: current_timestamp(),
            accumulator_value: vec![0u8; 32],
        });
        
        region_cache.executors.insert(peer_id.clone());
        region_cache.last_update = current_timestamp();
        
        tracing_info!("Registered peer {} in region {}", peer_id, region_id);
        
        Ok(peer_id)
    }
    
    /// Batch register peers with attestation information
    pub async fn batch_register_peers(
        &self,
        peers: Vec<(String, String, AttestationReport)>,
    ) -> Result<BatchRegistrationResult, TeeError> {
        if peers.is_empty() {
            return Ok(BatchRegistrationResult {
                success_count: 0,
                failed_count: 0,
                peers: Vec::new(),
            });
        }
        
        if peers.len() > self.params.max_peers_exchange {
            return Err(TeeError::ExecutionError("Batch size exceeds maximum".into()));
        }
        
        // In a real implementation, we would prepare the data and call batch_register_attestations
        // on the accumulator contract, which requires a Context from the contract execution environment
        // For now, we'll update the local cache to simulate the process
        
        // Update region caches for each unique region
        let mut cache = match self.region_cache.write() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire write lock on region_cache: {}", e);
                return Err(TeeError::ExecutionError(format!("Failed to acquire lock: {}", e)));
            }
        };
        
        let mut success_count = 0;
        let mut success_peers = Vec::new();
        
        for (peer_id, region_id, _) in &peers {
            let region_cache = cache.entry(region_id.clone())
                .or_insert_with(|| RegionCache {
                    executors: HashSet::new(),
                    last_update: current_timestamp(),
                    accumulator_value: vec![0u8; 32],
                });
            
            region_cache.executors.insert(peer_id.clone());
            region_cache.last_update = current_timestamp();
            success_count += 1;
            success_peers.push(peer_id.clone());
        }
        
        tracing_info!("Registered {} peers across {} regions", 
            peers.len(),
            peers.iter().map(|(_, region, _)| region).collect::<HashSet<_>>().len());
        
        Ok(BatchRegistrationResult {
            success_count: success_count as u64,
            failed_count: 0,
            peers: success_peers,
        })
    }
    
    /// Get all peers in a region
    pub async fn get_peers_by_region(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        // First get direct peers in this region
        let executors = self.get_region_peers(region_id).await?;
        Ok(executors)
    }
    
    /// Get all known regions
    pub async fn get_all_regions(&self) -> Result<Vec<DiscoveryRegionInfo>, TeeError> {
        // Call the accumulator client to get all regions
        match self.accumulator_client.get_all_regions().await {
            Ok(regions) => Ok(regions),
            Err(e) => {
                error!("Failed to get regions from accumulator: {}", e);
                Err(TeeError::ExecutionError(format!("Failed to get regions: {}", e)))
            }
        }
    }
    
    /// Batch verify executors
    pub async fn batch_verify_executors(&self, executors: Vec<String>) -> Result<Vec<bool>, TeeError> {
        if executors.is_empty() {
            return Ok(Vec::new());
        }
        
        if executors.len() > self.params.max_peers_exchange {
            return Err(TeeError::ExecutionError("Batch size exceeds maximum".into()));
        }
        
        // Call the accumulator client to verify the executors
        match self.accumulator_client.batch_verify_peers(&executors).await {
            Ok(results) => Ok(results),
            Err(e) => {
                error!("Failed to verify executors: {}", e);
                Err(TeeError::ExecutionError(format!("Failed to verify executors: {}", e)))
            }
        }
    }
    
    /// Refresh the cache from the accumulator
    pub async fn refresh_cache(&self) -> Result<(), TeeError> {
        let current_time = current_timestamp();
        let last_refresh = {
            let lock = match self.last_refresh.read() {
                Ok(lock) => lock,
                Err(_) => {
                    error!("Failed to acquire read lock on last_refresh");
                    return Err(TeeError::ExecutionError("Failed to acquire lock".into()));
                }
            };
            *lock
        };
        
        if current_time - last_refresh < self.params.heartbeat_interval_sec {
            return Ok(());
        }
        
        // Update the last refresh time
        match self.last_refresh.write() {
            Ok(mut lock) => {
                *lock = current_time;
            }
            Err(_) => {
                error!("Failed to acquire write lock on last_refresh");
                return Err(TeeError::ExecutionError("Failed to acquire lock".into()));
            }
        }
        
        // Refresh the peer cache
        let regions = match self.get_all_regions().await {
            Ok(regions) => regions,
            Err(e) => {
                error!("Failed to get regions: {}", e);
                return Err(e);
            }
        };
        
        for region in regions {
            match self.get_peers_by_region(&region.region_id).await {
                Ok(peers) => {
                    let mut cache = match self.region_cache.write() {
                        Ok(cache) => cache,
                        Err(_) => {
                            error!("Failed to acquire write lock on region_cache");
                            return Err(TeeError::ExecutionError("Failed to acquire lock".into()));
                        }
                    };
                    
                    let mut region_cache = cache.entry(region.region_id.clone()).or_insert_with(|| RegionCache {
                        executors: HashSet::new(),
                        last_update: current_timestamp(),
                        accumulator_value: vec![0u8; 32],
                    });
                    for peer in peers {
                        region_cache.add_executor(peer);
                    }
                }
                Err(e) => {
                    error!("Failed to get peers for region {}: {}", region.region_id, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Start the gossip service
    async fn start_gossip_background_task(self: Arc<Self>) -> Result<(), TeeError> {
        // Use a proper Arc<Self> to ensure 'static lifetime
        let discovery_service = self.clone();
        
        let gossip_enabled = match self.gossip_enabled.read() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        let gossip_interval = self.gossip_interval;
        
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(gossip_interval)).await;
                
                // Check if gossip is enabled
                let enabled = match discovery_service.gossip_enabled.read() {
                    Ok(guard) => *guard,
                    Err(e) => {
                        log::error!("Failed to read gossip_enabled flag");
                        continue;
                    }
                };
                
                if !enabled {
                    continue;
                }
                
                // Process the gossip queue
                if let Err(e) = process_gossip_queue(&discovery_service).await {
                    log::error!("Error processing gossip queue: {:?}", e);
                }
                
                // Send pings to super peers
                if let Err(e) = discovery_service.send_ping_to_super_peers().await {
                    log::error!("Error sending ping to super peers: {:?}", e);
                }
            }
        });
        
        Ok(())
    }
    
    /// Register a peer with a specific region and locality information
    pub async fn register_with_region_and_locality(
        &self,
        executor_id: String,
        region_id: String,
        attestation: AttestationReport,
        locality_info: Option<LocalityInfoDto>,
    ) -> Result<bool, TeeError> {
        // First register the peer with the region
        let result = self.register_peer(executor_id.clone(), region_id.clone(), attestation).await?;
        
        // If locality information is provided, store it
        if let Some(locality) = locality_info {
            let mut locality_map = match self.locality_info.write() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire write lock on locality_info: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            locality_map.insert(executor_id.clone(), locality.clone().into());
            
            // Check if gossip is enabled
            let gossip_enabled = match self.gossip_enabled.read() {
                Ok(guard) => *guard,
                Err(e) => {
                    log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            let should_gossip = gossip_enabled;
            
            // Drop locks before calling other async functions to avoid deadlocks
            drop(locality_map);
            drop(gossip_enabled);
            
            if should_gossip {
                self.queue_gossip_message(executor_id.clone(), GossipMessage::PeerAnnounce {
                    region_id,
                    executor_id,
                    locality: Some(locality),
                }).await?;
            }
        }
        
        // Result is successful if we got a non-empty string ID back
        Ok(!result.is_empty())
    }
    
    /// Set up hierarchical relationship between regions
    pub async fn set_region_hierarchy(
        &self,
        parent_region: String,
        child_region: String,
    ) -> Result<(), TeeError> {
        // Update the region hierarchy map
        {
            let mut region_hierarchy = match self.region_hierarchy.write() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire write lock on region_hierarchy: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            // Save the parent-child relationship
            region_hierarchy.insert(child_region.clone(), parent_region.clone());
        }
        
        // Update the region children map
        {
            let mut region_children = match self.region_children.write() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire write lock on region_children: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            let children = region_children.entry(parent_region.clone()).or_insert_with(Vec::new);
            if !children.contains(&child_region) {
                children.push(child_region.clone());
            }
        }
        
        // Propagate the hierarchy via gossip
        let gossip_enabled = match self.gossip_enabled.read() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        if gossip_enabled {
            self.queue_gossip_message(
                "broadcast".to_string(), 
                GossipMessage::RegionHierarchy { 
                    parent: parent_region,
                    child: child_region
                }
            ).await?;
        }
        
        Ok(())
    }
    
    /// Update proximity information between regions
    pub async fn update_region_proximity(
        &self,
        from_region: String,
        to_region: String,
        latency_ms: u32,
    ) -> Result<(), TeeError> {
        // Update the region proximity map
        {
            let mut region_proximity = match self.proximity_map.write() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire write lock on proximity_map: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            let entry = region_proximity.entry(from_region.clone()).or_insert_with(HashMap::new);
            entry.insert(to_region.clone(), latency_ms);
        }
        
        // Propagate the proximity information via gossip
        let gossip_enabled = match self.gossip_enabled.read() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        if gossip_enabled {
            self.queue_gossip_message(
                "broadcast".to_string(),
                GossipMessage::RegionProximityUpdate { 
                    from_region,
                    to_region,
                    proximity: latency_ms
                }
            ).await?;
        }
        
        Ok(())
    }
    
    /// Calculate the proximity between two regions
    async fn calculate_region_proximity(
        &self,
        region1: &str,
        region2: &str,
        parent_of_region1: Option<&String>,
    ) -> u32 {
        // Start with a high base distance
        let mut proximity = 1000u32;
        
        // Check if they share a parent-child relationship
        if let Some(parent) = parent_of_region1 {
            if parent == region2 {
                // Direct parent-child relationship
                proximity = 10;
            } else {
                // Check if they share a common ancestor
                let region_hierarchy = match self.region_hierarchy.read() {
                    Ok(rh) => rh,
                    Err(e) => {
                        log::error!("Failed to acquire read lock on region_hierarchy: {}", e);
                        return 1000; // Return high distance on error
                    }
                };
                if let Some(parent_of_region2) = region_hierarchy.get(region2) {
                    if parent_of_region2 == parent {
                        // Sibling regions
                        proximity = 20;
                    }
                }
            }
        }
        
        // Further refine based on locality information if available
        let locality_info = match self.locality_info.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on locality_info: {}", e);
                return proximity; // Return current proximity on error
            }
        };
        
        let locality1 = locality_info.get(region1).cloned();
        let locality2 = locality_info.get(region2).cloned();
        
        if let (Some(info1), Some(info2)) = (locality1, locality2) {
            // If in same geographic region, reduce distance
            if info1.region_id == info2.region_id {
                proximity = proximity.saturating_sub(5);
            }
            
            // If in same network zone, reduce distance further
            if info1.zone_id == info2.zone_id {
                proximity = proximity.saturating_sub(5);
            }
        }
        
        proximity
    }
    
    /// Find the nearest executors to a given region based on locality
    pub async fn find_nearest_executors(
        &self,
        from_region: &str,
        count: u64,
    ) -> Result<Vec<(String, u32)>, TeeError> {
        let mut result = Vec::new();
        
        // Get direct peers from the region
        let executors = self.get_peers_by_region(from_region).await?;
        for executor in executors {
            result.push((executor, 0)); // Direct peers have 0 distance
        }
        
        // If we need more, look at related regions
        if result.len() < count as usize {
            // Get proximity data
            let proximity_map = match self.proximity_map.read() {
                Ok(map) => map,
                Err(e) => {
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            // Find regions sorted by proximity to the source region
            let mut regions_by_proximity = Vec::new();
            
            if let Some(region_map) = proximity_map.get(from_region) {
                for (region, proximity) in region_map {
                    regions_by_proximity.push((region.to_string(), *proximity));
                }
                
                // Sort by proximity (ascending)
                regions_by_proximity.sort_by(|a, b| a.1.cmp(&b.1));
            }
            
            // Add executors from nearby regions until we reach the count
            for (region, proximity) in regions_by_proximity {
                if result.len() >= count as usize {
                    break;
                }
                
                let region_executors = self.get_peers_by_region(&region).await?;
                for executor in region_executors {
                    result.push((executor, proximity));
                    if result.len() >= count as usize {
                        break;
                    }
                }
            }
        }
        
        Ok(result)
    }
    
    /// Get peers in a region recursively (including child regions)
    pub async fn get_region_peers_recursive(
        &self,
        region_id: &str,
    ) -> Result<Vec<String>, TeeError> {
        // Use Box::pin to handle recursion in async function
        let mut result = Vec::new();
        
        // Get direct peers
        let direct_peers = self.get_region_peers(region_id).await?;
        result.extend(direct_peers);
        
        // Get children regions
        let children = match self.region_children.read() {
            Ok(children_lock) => children_lock.get(region_id).cloned(),
            Err(e) => {
                log::error!("Failed to acquire read lock on region_children: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        }.unwrap_or_default();
        
        // Process each child region using a helper function
        for child in children {
            let child_peers = self.get_region_peers_recursive_helper(&child).await?;
            result.extend(child_peers);
        }
        
        Ok(result)
    }
    
    // Helper function to handle recursive async calls
    async fn get_region_peers_recursive_helper(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        Box::pin(self.get_region_peers_recursive(region_id)).await
    }
    
    /// Get peers by proximity to a region
    pub async fn get_peers_by_proximity(&self, from_region: &str) -> Result<Vec<(String, Vec<String>)>, TeeError> {
        // Get the proximity map
        let proximity_map = match self.proximity_map.read() {
            Ok(map) => map,
            Err(e) => {
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        // Get the regions in proximity order
        let mut regions_by_proximity = Vec::new();
        
        if let Some(region_map) = proximity_map.get(from_region) {
            for (region, proximity) in region_map {
                regions_by_proximity.push((region.to_string(), *proximity));
            }
            
            // Sort by proximity value (ascending)
            regions_by_proximity.sort_by(|a, b| a.1.cmp(&b.1));
        }
        
        // Get peers for each region
        let mut result = Vec::new();
        
        for (region_name, _) in regions_by_proximity {
            let region_executors = self.get_peers_by_region(&region_name).await?;
            result.push((region_name, region_executors));
        }
        
        Ok(result)
    }
    
    /// Get all known regions with their hierarchical relationships
    pub async fn get_regions_with_hierarchy(&self) -> Result<Vec<HierarchicalRegionInfo>, TeeError> {
        // Get all regions
        let regions = {
            let region_cache = match self.region_cache.read() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire read lock on region_cache: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            region_cache.keys().cloned().collect::<Vec<_>>()
        };
        
        // Get region hierarchy information
        let hierarchy = {
            let region_hierarchy = match self.region_hierarchy.read() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire read lock on region_hierarchy: {}", e);
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            };
            
            region_hierarchy.clone()
        };
        
        // Get region children information
        let children = match self.region_children.read() {
            Ok(region_children) => region_children.clone(),
            Err(e) => {
                log::error!("Failed to acquire read lock on region_children: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        // Create HierarchicalRegionInfo for each region
        let mut result = Vec::with_capacity(regions.len());
        for region_id in regions.into_iter() {
            let region_str = region_id.to_string();  // Convert to String before using
            
            // Get the parent region from the hierarchy map
            let parent_region = hierarchy.get(&region_str).cloned();
            
            // Get the child regions from the children map
            let child_regions = children.get(&region_str).cloned().unwrap_or_default();
            
            // Get the number of executors in this region
            let executor_count = self.get_region_peers(&region_str).await?.len() as u64;
            
            result.push(HierarchicalRegionInfo {
                region_id: region_str,
                parent_region,
                child_regions,
                executor_count,
            });
        }
        
        Ok(result)
    }
    
    /// Get peers by region proximity
    pub async fn get_peers_by_region_proximity(&self, from_region: &str) -> Result<Vec<String>, TeeError> {
        // Get the proximity map
        let proximity_map = match self.proximity_map.read() {
            Ok(map) => map,
            Err(e) => {
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        // Get the regions in proximity order
        let mut regions_by_proximity = Vec::new();
        
        if let Some(region_map) = proximity_map.get(from_region) {
            for (region, proximity) in region_map {
                regions_by_proximity.push((region.to_string(), *proximity));
            }
            
            // Sort by proximity (ascending)
            regions_by_proximity.sort_by(|a, b| a.1.cmp(&b.1));
        }
        
        // Get executors for each region
        let mut executors = Vec::new();
        
        for (region_id, _proximity) in regions_by_proximity {
            let region_executors = self.get_peers_by_region(&region_id).await?;
            executors.extend(region_executors);
        }
        
        // Get direct executors in this region
        let direct_executors = self.get_peers_by_region(from_region).await?;
        executors.extend(direct_executors);
        
        // Remove duplicates
        executors.sort();
        executors.dedup();
        
        Ok(executors)
    }
    
    /// Remove a peer from super peers for a region
    pub async fn remove_super_peer(
        &self,
        region_id: String,
        peer_id: &str,
    ) -> Result<(), TeeError> {
        let mut super_peers = match self.super_peers.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on super_peers: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        if let Some(peers) = super_peers.get_mut(&region_id) {
            peers.remove(peer_id);
        }
        
        // Remove from last seen map
        let mut super_peers_last_seen = match self.super_peers_last_seen.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on super_peers_last_seen: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        super_peers_last_seen.remove(peer_id);
        
        // If gossip is enabled, announce this super peer removal
        let gossip_enabled = match self.gossip_enabled.read() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        let should_gossip = gossip_enabled;
        
        // Drop locks before calling other async functions to avoid deadlocks
        drop(super_peers);
        drop(super_peers_last_seen);
        drop(gossip_enabled);
        
        if should_gossip {
            self.queue_gossip_message(
                "broadcast".to_string(),
                GossipMessage::SuperPeerRemove {
                    region_id,
                    executor_id: peer_id.to_string(),
                }
            ).await?;
        }
        
        Ok(())
    }
    
    /// Get super peers for a region
    pub async fn get_super_peers(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        let super_peers = match self.super_peers.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on super_peers: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        // Use a let binding to create a longer lived empty HashSet if needed
        let empty_set = HashSet::new();
        let peers = super_peers.get(region_id).unwrap_or(&empty_set);
        Ok(peers.iter().cloned().collect())
    }
    
    /// Check if a peer is currently registered as a super peer in any region
    pub async fn is_super_peer(&self, peer_id: &str) -> Result<bool, TeeError> {
        let super_peers = match self.super_peers.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on super_peers: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        for peers in super_peers.values() {
            if peers.contains(peer_id) {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Enable or disable the gossip protocol
    pub async fn set_gossip_enabled(&self, enabled: bool) -> Result<(), TeeError> {
        let mut gossip_enabled = match self.gossip_enabled.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        *gossip_enabled = enabled;
        
        Ok(())
    }
    
    /// Check if a peer is alive by sending a ping message
    pub async fn ping_peer(&self, peer_id: &str) -> Result<bool, TeeError> {
        // In a real implementation, this would send an actual network request
        // For this implementation, we'll check if the peer is in our cache
        
        let regions = match self.region_cache.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on region cache: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        // Check if peer exists in any region
        let mut peer_exists = false;
        for (_, peers) in regions.iter() {
            if peers.executors.contains(&peer_id.to_string()) {
                peer_exists = true;
                break;
            }
        }
        
        // If the peer exists in our cache, queue a ping message
        if peer_exists {
            self.queue_gossip_message(peer_id.to_string(), GossipMessage::Ping).await?;
            
            // In a real implementation, we would wait for the response
            // For this implementation, we'll just return true if the peer exists
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Handle a pong response from a peer
    pub async fn handle_pong(&self, executor_id: &str) -> Result<(), TeeError> {
        // Update the last seen timestamp
        let mut super_peers_last_seen = match self.super_peers_last_seen.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on super_peers_last_seen: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        super_peers_last_seen.insert(executor_id.to_string(), Instant::now());
        
        Ok(())
    }
    
    /// Send ping to all super peers to maintain connectivity
    pub async fn send_ping_to_super_peers(&self) -> Result<(), TeeError> {
        // Get all known super peers
        let all_super_peers = {
            match self.super_peers.read() {
                Ok(super_peers) => {
                    super_peers.values()
                        .flat_map(|peers| peers.iter().cloned())
                        .collect::<HashSet<String>>()
                },
                Err(e) => {
                    return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                }
            }
        };
        
        // Queue ping messages
        for peer in all_super_peers {
            let _ = self.queue_gossip_message(peer, GossipMessage::Ping).await;
        }
        
        Ok(())
    }
    
    /// Set a peer as a super peer for a region
    pub async fn set_super_peer(
        &self,
        region_id: String,
        peer_id: String,
    ) -> Result<(), TeeError> {
        let mut super_peers = match self.super_peers.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on super_peers: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        let peers = super_peers.entry(region_id.clone()).or_insert_with(HashSet::new);
        peers.insert(peer_id.clone());
        
        // Initialize last seen timestamp
        let mut super_peers_last_seen = match self.super_peers_last_seen.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on super_peers_last_seen: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        super_peers_last_seen.insert(peer_id.clone(), Instant::now());
        
        // If gossip is enabled, announce this super peer
        let gossip_enabled = match self.gossip_enabled.read() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on gossip_enabled: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        let should_gossip = gossip_enabled;
        
        // Drop locks before making async calls
        drop(super_peers);
        drop(super_peers_last_seen);
        drop(gossip_enabled);
        
        if should_gossip {
            self.queue_gossip_message(
                "broadcast".to_string(),
                GossipMessage::SuperPeerAnnounce {
                    region_id,
                    executor_id: peer_id,
                }
            ).await?;
        }
        
        Ok(())
    }
    
    /// Queue a gossip message to be sent to all peers
    pub async fn queue_gossip_message(&self, peer: String, message: GossipMessage) -> Result<(), TeeError> {
        let mut queue = match self.gossip_queue.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire lock on gossip queue: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        queue.push_back((peer, message));
        Ok(())
    }
    
    /// Get direct peers in a region (non-recursive)
    pub async fn get_region_peers(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        let region_cache = match self.region_cache.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on region_cache: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        match region_cache.get(region_id) {
            Some(cache) => Ok(cache.executors.iter().cloned().collect()),
            None => Ok(Vec::new())
        }
    }
    
    /// Get peers in proximity to a region
    pub async fn get_peers_in_proximity(&self, from_region: &str) -> Result<Vec<String>, TeeError> {
        // Just delegate to the new implementation
        self.get_peers_by_region_proximity(from_region).await
    }
    
    /// Get proximity between two regions
    pub async fn get_proximity(&self, region1: &str, region2: &str) -> Result<u32, TeeError> {
        // Delegate to the more comprehensive estimate_region_latency method
        self.estimate_region_latency(region1, region2).await
    }
    
    /// Estimate latency between two regions
    pub async fn estimate_region_latency(&self, region1: &str, region2: &str) -> Result<u32, TeeError> {
        if region1 == region2 {
            return Ok(0); // Same region, no latency
        }
        
        // Check if we have a direct measurement
        let proximity_map = match self.proximity_map.read() {
            Ok(map) => map,
            Err(e) => {
                error!("Failed to acquire read lock on proximity_map: {}", e);
                return Err(TeeError::ExecutionError(format!("Failed to acquire lock: {}", e)));
            }
        };
        
        if let Some(region_map) = proximity_map.get(region1) {
            if let Some(latency) = region_map.get(region2) {
                return Ok(*latency);
            }
        }
        
        // Try the reverse direction
        if let Some(region_map) = proximity_map.get(region2) {
            if let Some(latency) = region_map.get(region1) {
                return Ok(*latency);
            }
        }
        
        // If no direct entry, check if the regions are siblings or have a common ancestor
        let region_hierarchy = match self.region_hierarchy.read() {
            Ok(rh) => rh,
            Err(e) => {
                error!("Failed to acquire read lock on region_hierarchy: {}", e);
                return Err(TeeError::ExecutionError(format!("Failed to acquire lock: {}", e)));
            }
        };
        
        // Check if they are direct siblings (same parent)
        let parent1 = region_hierarchy.get(region1);
        let parent2 = region_hierarchy.get(region2);
        
        if let (Some(p1), Some(p2)) = (parent1, parent2) {
            if p1 == p2 {
                // They are siblings, return a default sibling proximity (e.g., 10)
                return Ok(10);
            }
        }
        
        // Check for common ancestors and calculate proximity based on "distance" to common ancestor
        // This is a simplified implementation for now
        
        // If no direct measurement, use a default high value
        // In a real implementation, we could use more sophisticated estimation
        Ok(1000) // Default high latency
    }
    
    /// Send a discovery message to the specified executor
    pub async fn send_discovery_message(&self, executor_id: &str, message: DiscoveryMessage) -> Result<(), TeeError> {
        // Implement the actual message sending logic
        // For now, we'll just log the action
        tracing_info!("Sending discovery message to {}: {:?}", executor_id, message);
        Ok(())
    }
    
    /// Check if a region is a parent of another region
    pub async fn is_parent_region(&self, parent: &str, child: &str) -> Result<bool, TeeError> {
        // Get the region hierarchy map
        let region_hierarchy = match self.region_hierarchy.read() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire read lock on region_hierarchy: {}", e);
                return Err(TeeError::ExecutionError(format!("Failed to acquire lock: {}", e)));
            }
        };
        
        // Check if parent is directly the parent of child
        if let Some(parent_of_child) = region_hierarchy.get(child) {
            return Ok(parent_of_child == parent);
        }
        
        Ok(false)
    }
    
    /// Get the best peer for a region based on connection health and locality
    pub async fn get_best_peer_for_region(&self, region_id: &str, 
                                     local_locality: Option<&LocalityInfoDto>) -> Result<Option<String>, TeeError> {
        let region_cache = match self.region_cache.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on region cache: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        // If we have super peers for this region, prefer them
        let super_peers = self.get_super_peers(region_id).await?;
        if !super_peers.is_empty() {
            // Find the best super peer based on locality and health
            let mut best_peer = "";
            let mut best_score = f64::NEG_INFINITY;
            
            for peer_id in &super_peers {
                let health = self.get_connection_health(peer_id).await?;
                let is_super_peer = self.is_super_peer(peer_id).await?;
                
                // Super peers always get a high score to ensure they are kept
                let mut score = if is_super_peer { 
                    health + 10.0 
                } else { 
                    health 
                };
                
                // Add locality bonus if available
                let connection_stats = match self.connection_stats.read() {
                    Ok(guard) => guard,
                    Err(e) => {
                        log::error!("Failed to acquire read lock on connection stats: {}", e);
                        return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                    }
                };
                
                if let (Some(local), Some(stats)) = (local_locality, connection_stats.get(peer_id)) {
                    if let Some(remote) = &stats.locality {
                        // Simple locality scoring: same region gets a bonus
                        if local.region_id == remote.region_id {
                            score += 5.0;
                        }
                        // Same country gets a smaller bonus
                        if local.zone_id == remote.zone_id {
                            score += 2.0;
                        }
                    }
                }
                
                if score > best_score {
                    best_score = score;
                    best_peer = peer_id;
                }
            }
            
            if !best_peer.is_empty() {
                return Ok(Some(best_peer.to_string()));
            }
        }
        
        // If no super peers or no suitable super peer found, fall back to any peer in the region
        if let Some(peers) = region_cache.get(region_id) {
            if peers.executors.is_empty() {
                return Ok(None);
            }
            
            // Choose the best peer based on health
            let mut best_peer = "";
            let mut best_health = 0.0;
            
            for peer_id in &peers.executors {
                let health = self.get_connection_health(peer_id).await?;
                if health > best_health {
                    best_health = health;
                    best_peer = peer_id;
                }
            }
            
            if !best_peer.is_empty() {
                return Ok(Some(best_peer.to_string()));
            }
        }
        
        Ok(None)
    }
    
    /// Balance connections across regions
    pub async fn balance_connections(&self, max_connections_per_region: usize) -> Result<(), TeeError> {
        // Get all regions
        let region_cache = match self.region_cache.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on region cache: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        for (region_id, peers) in region_cache.iter() {
            if peers.executors.len() <= max_connections_per_region {
                continue;
            }
            
            // Too many connections for this region, select the best ones to keep
            let mut peer_scores = Vec::new();
            
            for peer_id in &peers.executors {
                let health = self.get_connection_health(peer_id).await?;
                let is_super_peer = self.is_super_peer(peer_id).await?;
                
                // Super peers always get a high score to ensure they are kept
                let score = if is_super_peer { 
                    health + 10.0 
                } else { 
                    health 
                };
                
                peer_scores.push((peer_id.clone(), score));
            }
            
            // Sort by score descending
            peer_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            
            // Keep the top max_connections_per_region peers, disconnect the rest
            if peer_scores.len() > max_connections_per_region {
                for (peer_id, _) in peer_scores.iter().skip(max_connections_per_region) {
                    log::info!("Balancing connections for region {}: disconnecting from peer {}", region_id, peer_id);
                    
                    // In a real implementation, this would actually disconnect
                    // For now, just update the stats to reflect the disconnection
                    self.update_connection_failure(peer_id).await?;
                }
            }
        }
        
        Ok(())
    }

    /// Get connection health for a peer (0.0-1.0)
    pub async fn get_connection_health(&self, peer_id: &str) -> Result<f64, TeeError> {
        let connections = match self.connection_stats.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire read lock on connection_stats: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        if let Some(stats) = connections.get(peer_id) {
            let reliability = stats.reliability_score();
            
            // Factor in recency of interaction
            let elapsed = stats.last_used.elapsed();
            let recency_factor = 1.0 / (1.0 + (elapsed.as_secs() as f64 / 3600.0)); // Diminish score for older connections
            
            return Ok(reliability * 0.7 + recency_factor * 0.3); // Weight reliability higher than recency
        }
        
        // Default health for unknown peers
        Ok(0.5)
    }
    
    /// Update connection stats after a successful message
    pub async fn update_connection_success(&self, peer_id: &str, response_time_ms: f64) -> Result<(), TeeError> {
        let mut connections = match self.connection_stats.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on connection_stats: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        if let Some(stats) = connections.get_mut(peer_id) {
            stats.update_success(response_time_ms);
        } else {
            // If we don't have stats for this peer yet, initialize them
            connections.insert(peer_id.to_string(), ConnectionStats::new(None));
        }
        
        Ok(())
    }
    
    /// Update connection stats after a failed message
    pub async fn update_connection_failure(&self, peer_id: &str) -> Result<(), TeeError> {
        let mut connections = match self.connection_stats.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on connection_stats: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        if let Some(stats) = connections.get_mut(peer_id) {
            stats.update_failure();
        } else {
            // If we don't have stats for this peer yet, initialize them with a failure
            let mut new_stats = ConnectionStats::new(None);
            new_stats.update_failure();
            connections.insert(peer_id.to_string(), new_stats);
        }
        
        Ok(())
    }
    
    /// Initialize connection stats for a peer
    pub async fn init_connection_stats(&self, peer_id: &str, locality: Option<LocalityInfoDto>) -> Result<(), TeeError> {
        let mut connections = match self.connection_stats.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on connection_stats: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        connections.insert(peer_id.to_string(), ConnectionStats::new(locality));
        Ok(())
    }
    
    /// Prune inactive connections
    pub async fn prune_inactive_connections(&self, max_inactive_time: Duration) -> Result<usize, TeeError> {
        let now = Instant::now();
        let mut connections = match self.connection_stats.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire write lock on connection_stats: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        let mut to_remove = Vec::new();
        
        // Identify inactive connections
        for (peer_id, stats) in connections.iter() {
            if now.duration_since(stats.last_used) > max_inactive_time {
                to_remove.push(peer_id.clone());
            }
        }
        
        // Remove inactive connections
        let pruned_count = to_remove.len();
        for peer_id in to_remove {
            connections.remove(&peer_id);
            log::info!("Pruned inactive connection to peer {}", peer_id);
        }
        
        log::info!("Pruned {} inactive connections", pruned_count);
        Ok(pruned_count)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalRegionInfo {
    /// Region ID
    pub region_id: String,
    
    /// Parent region if any
    pub parent_region: Option<String>,
    
    /// Child regions if any
    pub child_regions: Vec<String>,
    
    /// Number of executors in this region
    pub executor_count: u64,
}

/// Process the gossip message queue
async fn process_gossip_queue(discovery_service: &DiscoveryService) -> Result<(), TeeError> {
    // Get messages from the queue (up to 10 at a time)
    let messages = {
        let mut queue = match discovery_service.gossip_queue.lock() {
            Ok(q) => q,
            Err(e) => {
                log::error!("Failed to acquire lock on gossip queue: {}", e);
                return Err(TeeError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        };
        
        let mut batch = Vec::new();
        for _ in 0..10 {
            if let Some(msg) = queue.pop_front() {
                batch.push(msg);
            } else {
                break;
            }
        }
        batch
    };
    
    // Process each message
    for (peer, message) in messages {
        match message {
            GossipMessage::PeerAnnounce { region_id, executor_id, locality } => {
                log::debug!("Received PeerAnnounce message for executor {} in region {}", executor_id, region_id);
                
                // Update regions
                {
                    if let Ok(mut regions) = discovery_service.region_cache.write() {
                        if !regions.contains_key(&region_id) {
                            regions.insert(region_id.clone(), RegionCache {
                                executors: HashSet::new(),
                                last_update: current_timestamp(),
                                accumulator_value: vec![0u8; 32],
                            });
                        }
                        
                        if let Some(cache) = regions.get_mut(&region_id) {
                            if !cache.executors.contains(&executor_id) {
                                cache.executors.insert(executor_id.clone());
                            }
                        }
                    } else {
                        log::error!("Failed to acquire write lock on region_cache for PeerAnnounce");
                    }
                }
                
                // Store locality information if provided
                if let Some(loc_info) = locality {
                    if let Ok(mut locality_map) = discovery_service.locality_info.write() {
                        locality_map.insert(executor_id.clone(), loc_info.clone().into());
                    } else {
                        log::error!("Failed to acquire write lock on locality_info for PeerAnnounce");
                    }
                }
            },
            GossipMessage::RegionHierarchy { parent, child } => {
                log::debug!("Received RegionHierarchy message: parent={}, child={}", parent, child);
                
                // Update our knowledge of region hierarchy
                {
                    if let Ok(mut region_hierarchy) = discovery_service.region_hierarchy.write() {
                        // Add to our region hierarchy
                        region_hierarchy.insert(child.clone(), parent.clone());
                        log::debug!("Updated region hierarchy: {:?}", region_hierarchy);
                    } else {
                        log::error!("Failed to acquire write lock on region_hierarchy");
                    }
                    
                    if let Ok(mut region_children) = discovery_service.region_children.write() {
                        // Update children list
                        let children_set = region_children.entry(parent.clone()).or_insert_with(Vec::new);
                        if !children_set.contains(&child) {
                            children_set.push(child.clone());
                        }
                        log::debug!("Updated region children: {:?}", region_children);
                    } else {
                        log::error!("Failed to acquire write lock on region_children");
                    }
                }
            },
            GossipMessage::Ping => {
                log::debug!("Received Ping message from {}", peer);
                // Respond with a Pong
                discovery_service.queue_gossip_message(peer.clone(), GossipMessage::Pong { executor_id: peer.clone() }).await?;
            },
            GossipMessage::Pong { executor_id } => {
                log::debug!("Received Pong message from {}", executor_id);
                // Update last seen timestamp
                if let Ok(mut super_peers_last_seen) = discovery_service.super_peers_last_seen.write() {
                    super_peers_last_seen.insert(executor_id.clone(), Instant::now());
                } else {
                    log::error!("Failed to acquire write lock on super_peers_last_seen");
                }
            },
            GossipMessage::SuperPeerAnnounce { region_id, executor_id } => {
                log::debug!("Received SuperPeerAnnounce message for executor {} in region {}", executor_id, region_id);
                
                // Add super peer to our registry
                if let Ok(mut super_peers) = discovery_service.super_peers.write() {
                    let super_peer_set = super_peers.entry(region_id.clone()).or_default();
                    super_peer_set.insert(executor_id.clone());
                } else {
                    log::error!("Failed to acquire write lock on super_peers");
                }
                
                // Initialize last seen timestamp
                if let Ok(mut super_peers_last_seen) = discovery_service.super_peers_last_seen.write() {
                    super_peers_last_seen.insert(executor_id.clone(), Instant::now());
                } else {
                    log::error!("Failed to acquire write lock on super_peers_last_seen");
                }
            },
            GossipMessage::SuperPeerRemove { region_id, executor_id } => {
                log::info!(
                    "Received super peer remove: {} in region {}",
                    executor_id, region_id
                );
                
                // Remove from super peers list
                {
                    if let Ok(mut super_peers) = discovery_service.super_peers.write() {
                        if let Some(peers) = super_peers.get_mut(&region_id) {
                            peers.remove(&executor_id);
                            
                            // Remove region if empty
                            if peers.is_empty() {
                                super_peers.remove(&region_id);
                            }
                        }
                    } else {
                        log::error!("Failed to acquire write lock on super_peers for SuperPeerRemove");
                    }
                }
                
                // Remove from last seen
                {
                    if let Ok(mut super_peers_last_seen) = discovery_service.super_peers_last_seen.write() {
                        super_peers_last_seen.remove(&executor_id);
                    } else {
                        log::error!("Failed to acquire write lock on super_peers_last_seen for SuperPeerRemove");
                    }
                }
            },
            GossipMessage::RegionProximityUpdate { from_region, to_region, proximity } => {
                log::debug!("Received RegionProximityUpdate message: {} to {} with proximity {}", from_region, to_region, proximity);
                
                // Update proximity information
                if let Ok(mut proximity_map) = discovery_service.proximity_map.write() {
                    let entry = proximity_map.entry(from_region.clone()).or_insert_with(HashMap::new);
                    entry.insert(to_region.clone(), proximity);
                    log::debug!("Updated proximity map: {:?}", proximity_map);
                } else {
                    log::error!("Failed to acquire write lock on proximity_map");
                }
            },
            GossipMessage::RegionUpdate { .. } => {
                log::debug!("Received RegionUpdate message - not implemented yet");
            },
            GossipMessage::RoutingTable { .. } => {
                log::debug!("Received RoutingTable message - not implemented yet");
            },
        }
    }
    
    // Return success
    Ok(())
}
