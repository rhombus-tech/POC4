use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tonic::{Request, Response, Status};
use tonic::transport::{Channel, Server};
use tonic::service::interceptor::InterceptedService;
use tracing::{debug, error, info, warn};
use std::net::SocketAddr;
use chrono::Utc;
use tee_interface::RegionInfo;

use crate::proto::teeservice::{
    ExecutionRequest, ExecutionResult, 
    tee_execution_server::{TeeExecution, TeeExecutionServer},
    tee_execution_client::TeeExecutionClient,
    GetRegionsRequest, GetRegionsResponse,
    GetAttestationsRequest, RegionAttestations,
    Region, TeeAttestation,
    DeployContractRequest, DeployContractResponse,
};

/// Represents information about a TEE peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub address: String,
    pub region_id: String,
    pub role: String,
}

/// Represents information about a TEE peer
#[derive(Debug, Clone)]
pub struct TeeInfo {
    /// Unique identifier for the TEE
    pub id: String,
    /// Network address of the TEE peer
    pub address: SocketAddr,
    /// Region identifier this TEE belongs to
    pub region_id: String,
    /// Last time this peer was seen (Unix timestamp)
    pub last_seen: u64,
    /// Role of this TEE in the pair
    pub role: TeeRole,
}

/// Role of a TEE in a pair
#[derive(Debug, Clone, PartialEq)]
pub enum TeeRole {
    Primary,
    Secondary,
    Unknown,
}

impl ToString for TeeRole {
    fn to_string(&self) -> String {
        match self {
            TeeRole::Primary => "primary".to_string(),
            TeeRole::Secondary => "secondary".to_string(),
            TeeRole::Unknown => "unknown".to_string(),
        }
    }
}

/// Type for storing TEE peer information
pub type TeeRegistry = Arc<RwLock<HashMap<String, TeeInfo>>>;

/// Service for managing direct TEE-to-TEE communication
pub struct TeePeerService {
    /// This TEE's unique identifier
    tee_id: String,
    /// This TEE's region identifier
    region_id: String,
    /// Registry of known TEE peers
    peers: TeeRegistry,
    /// Channel for sending execution requests to the TEE controller
    execution_tx: mpsc::Sender<(ExecutionRequest, mpsc::Sender<Result<ExecutionResult, Status>>)>,
}

impl TeePeerService {
    /// Create a new TeePeerService
    pub fn new(
        tee_id: String,
        region_id: String,
        execution_tx: mpsc::Sender<(ExecutionRequest, mpsc::Sender<Result<ExecutionResult, Status>>)>,
    ) -> Self {
        TeePeerService {
            tee_id,
            region_id,
            peers: Arc::new(RwLock::new(HashMap::new())),
            execution_tx,
        }
    }

    /// Register a new TEE peer
    pub async fn register_peer(&self, peer_info: TeeInfo) -> Result<(), String> {
        let mut peers = self.peers.write().await;
        info!("Registering peer: {:?}", peer_info);
        peers.insert(peer_info.id.clone(), peer_info);
        Ok(())
    }

    /// Find a peer by region and role
    pub async fn find_peer_by_region_and_role(&self, region_id: &str, role: TeeRole) -> Option<TeeInfo> {
        let peers = self.peers.read().await;
        peers.values()
            .find(|peer| peer.region_id == region_id && peer.role == role)
            .cloned()
    }

    /// Start the peer discovery and communication service
    pub async fn start(self, address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let peer_service = Arc::new(self);
        let service_clone = peer_service.clone();

        let handler = TeePeerServiceHandler {
            inner: peer_service,
        };

        // Spawn a task to periodically check for and clean up stale peers
        tokio::spawn(async move {
            service_clone.maintain_peer_registry().await;
        });

        // Create the server with our service implementation
        info!("Starting TEE peer service on {}", address);
        Server::builder()
            .add_service(TeeExecutionServer::new(handler))
            .serve(address)
            .await?;

        Ok(())
    }

    /// Maintain the peer registry by removing stale peers
    async fn maintain_peer_registry(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = self.cleanup_stale_peers().await {
                warn!("Error cleaning up stale peers: {}", e);
            }
        }
    }

    /// Remove peers that haven't been seen recently
    async fn cleanup_stale_peers(&self) -> Result<(), String> {
        let now = Utc::now().timestamp() as u64;
        let stale_threshold = 60; // seconds
        
        let mut to_remove = Vec::new();
        
        // Find stale peers
        let peers = self.peers.read().await;
        for (id, peer) in peers.iter() {
            if now - peer.last_seen > stale_threshold {
                to_remove.push(id.clone());
            }
        }
        drop(peers); // Release the read lock
        
        // Remove stale peers
        let mut peers = self.peers.write().await;
        for id in to_remove.iter() {
            peers.remove(id);
            info!("Removed stale peer: {}", id);
        }
        
        Ok(())
    }

    /// Send an execution request to a peer
    pub async fn execute_on_peer(
        &self,
        peer_address: SocketAddr,
        payload: ExecutionRequest,
    ) -> Result<ExecutionResult, Status> {
        debug!("Connecting to peer at {}", peer_address);
        
        let channel = match Channel::builder(format!("http://{}", peer_address).parse().unwrap())
            .connect()
            .await {
                Ok(channel) => channel,
                Err(e) => {
                    error!("Failed to connect to peer at {}: {}", peer_address, e);
                    return Err(Status::unavailable(format!("Failed to connect to peer: {}", e)));
                }
            };

        let mut client = TeeExecutionClient::new(channel);

        let request = tonic::Request::new(payload);

        debug!("Sending execution request to peer");
        let response = client.execute(request).await?;
        
        Ok(response.into_inner())
    }

    /// Get a list of peers in a specific region
    pub async fn get_peers_in_region(&self, region_id: &str) -> Vec<TeeInfo> {
        let peers = self.peers.read().await;
        peers.values()
            .filter(|peer| peer.region_id == region_id)
            .cloned()
            .collect()
    }
}

/// Handler for the TEE peer service
#[derive(Clone)]
struct TeePeerServiceHandler {
    inner: Arc<TeePeerService>,
}

#[tonic::async_trait]
impl TeeExecution for TeePeerServiceHandler {
    async fn execute(
        &self,
        request: Request<ExecutionRequest>
    ) -> Result<Response<ExecutionResult>, Status> {
        let execution_request = request.into_inner();
        
        // Create a channel for receiving the execution result
        let (result_tx, mut result_rx) = mpsc::channel(1);
        
        // Forward the request to the TEE controller
        if let Err(e) = self.inner.execution_tx.send((execution_request.clone(), result_tx)).await {
            return Err(Status::internal(format!("Failed to process execution request: {}", e)));
        }
        
        // Wait for the result
        match result_rx.recv().await {
            Some(Ok(result)) => {
                Ok(Response::new(result))
            }
            Some(Err(status)) => Err(status),
            None => Err(Status::internal("Execution channel closed unexpectedly")),
        }
    }

    async fn get_regions(
        &self,
        _request: Request<GetRegionsRequest>
    ) -> Result<Response<GetRegionsResponse>, Status> {
        // Simply return the regions we know about based on peers
        let peers = self.inner.peers.read().await;
        let mut regions = HashMap::new();
        
        for peer in peers.values() {
            regions.entry(peer.region_id.clone()).or_insert_with(|| Region {
                id: peer.region_id.clone(),
                created_at: Utc::now().timestamp().to_string(),
                worker_ids: vec![peer.id.clone()],
                supported_tee_types: vec!["SGX".to_string()],
                max_tasks: 100,
            });
        }
        
        let response = GetRegionsResponse {
            regions: regions.into_values().collect(),
        };
        
        Ok(Response::new(response))
    }

    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>
    ) -> Result<Response<RegionAttestations>, Status> {
        let req = request.into_inner();
        let region_id = req.region_id;
        
        // Get peers in this region
        let peers = self.inner.get_peers_in_region(&region_id).await;
        
        // Create attestations based on the protobuf definition
        let attestations = peers.into_iter().map(|peer| {
            TeeAttestation {
                enclave_id: peer.id.as_bytes().to_vec(),
                measurement: vec![],  // Mock data
                timestamp: Utc::now().timestamp().to_string(),
                data: vec![],         // Mock data
                signature: vec![],    // Mock data
                region_proof: vec![], // Mock data
                enclave_type: "SGX".to_string(), // Default type
            }
        }).collect();
        
        let response = RegionAttestations {
            attestations,
        };
        
        Ok(Response::new(response))
    }

    async fn deploy_contract(
        &self,
        request: Request<DeployContractRequest>
    ) -> Result<Response<DeployContractResponse>, Status> {
        let _req = request.into_inner();
        
        // Create a mock attestation for the contract deployment
        let attestation = TeeAttestation {
            enclave_id: self.inner.tee_id.as_bytes().to_vec(),
            measurement: vec![],  // Mock data
            timestamp: Utc::now().timestamp().to_string(),
            data: vec![],         // Mock data
            signature: vec![],    // Mock data
            region_proof: vec![], // Mock data
            enclave_type: "SGX".to_string(), // Default type
        };
        
        // For now, just return a mock successful response
        let response = DeployContractResponse {
            contract_id: format!("contract-{}", Utc::now().timestamp()),
            timestamp: Utc::now().timestamp().to_string(),
            attestations: vec![attestation],
        };
        
        Ok(Response::new(response))
    }
}
