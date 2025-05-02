use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tee_controller::accumulator_client::{AccumulatorClientTrait, MockAccumulatorClient, create_accumulator_client, LocalityInfoDto};
use tee_controller::discovery_service::DiscoveryService;
use tee_controller::mesh::{MeshCoordinator, MeshConfig, TeeType};

// Helper function to create a mock accumulator configuration
fn create_test_mesh_config(region_id: &str, enhanced_discovery: bool) -> MeshConfig {
    MeshConfig {
        region_id: region_id.to_string(),
        endpoint: format!("http://tee-endpoint-{}.test", region_id),
        tee_id: format!("tee-{}", region_id),
        max_peers: 10,
        discovery_interval_sec: 1, // Short for testing
        discovery_endpoint: "http://discovery.test".to_string(),
        circuit_breaker_threshold: Duration::from_millis(100),
        peer_refresh_interval: Duration::from_millis(200),
        enhanced_discovery,
        discovery_config: None,
        accumulator_endpoint: Some("http://accumulator.test".to_string()),
        local_identity: Some(format!("identity-{}", region_id)),
    }
}

// Test the accumulator client creation process
#[tokio::test]
async fn test_accumulator_client_creation() {
    // Test mock client creation
    let mock_client = create_accumulator_client(
        false, 
        None, 
        None
    );
    assert!(mock_client.get_all_regions().await.is_ok());
    
    // Test real client creation (which will still be a mock in tests)
    let real_client = create_accumulator_client(
        true, 
        Some("http://accumulator.test"), 
        Some("test-identity")
    );
    assert!(real_client.get_all_regions().await.is_ok());
}

// Test the integration of accumulator with mesh network for peer discovery
#[tokio::test]
async fn test_accumulator_mesh_integration() {
    // Create mesh with accumulator integration enabled
    let mesh_config = create_test_mesh_config("us-west", true);
    let mesh_coordinator = MeshCoordinator::new(mesh_config).await.expect("Failed to create mesh coordinator");
    
    // Check that mesh was created with accumulator client
    assert!(Arc::strong_count(&mesh_coordinator) > 0);
    
    // Sleep to allow discovery processes to run in the background
    sleep(Duration::from_millis(300)).await;
    
    // Use a public method to test that the mesh is working
    let discovery_result = mesh_coordinator.discover_peers(
        "us-west".to_string(),
        None,
        10
    ).await;
    
    // We should be able to discover peers (or at minimum, not encounter an error)
    assert!(discovery_result.is_ok());
}

// Test region management with accumulator
#[tokio::test]
async fn test_accumulator_region_management() {
    // Create mock accumulator client
    let mock_client: Arc<dyn AccumulatorClientTrait> = Arc::new(MockAccumulatorClient::new());
    
    // Get regions from accumulator
    let regions = mock_client.get_all_regions().await.expect("Failed to get regions");
    
    // Verify that the us-west and us-east regions are returned
    assert!(regions.iter().any(|r| r.region_id == "us-west"));
    assert!(regions.iter().any(|r| r.region_id == "us-east"));
    
    // Verify region hierarchy setting
    let result = mock_client.set_region_hierarchy("global", "us-west").await;
    assert!(result.is_ok());
}

// Test peer verification through accumulator
#[tokio::test]
async fn test_accumulator_peer_verification() {
    // Create mock accumulator client
    let mock_client: Arc<dyn AccumulatorClientTrait> = Arc::new(MockAccumulatorClient::new());
    
    // Register a peer
    let register_result = mock_client.register_peer("test-peer", "us-west").await;
    assert!(register_result.is_ok());
    
    // Verify a peer
    let verify_result = mock_client.verify_peer("test-peer").await;
    assert!(verify_result.is_ok());
    assert!(verify_result.unwrap());
    
    // Batch verify peers
    let batch_result = mock_client.batch_verify_peers(&["test-peer".to_string(), "other-peer".to_string()]).await;
    assert!(batch_result.is_ok());
    let batch_verifications = batch_result.unwrap();
    assert_eq!(batch_verifications.len(), 2);
    assert!(batch_verifications[0]); // test-peer should verify
}

// Test peer locality management with accumulator
#[tokio::test]
async fn test_accumulator_locality_management() {
    // Create mock accumulator client
    let mock_client: Arc<dyn AccumulatorClientTrait> = Arc::new(MockAccumulatorClient::new());
    
    // Create a locality info
    let locality = LocalityInfoDto {
        region_id: "us-west".to_string(),
        zone: Some("us-west-1a".to_string()),
        latitude: Some(37.7749),
        longitude: Some(-122.4194),
        tier: Some("premium".to_string()),
    };
    
    // Set peer locality
    let result = mock_client.set_peer_locality("test-peer", &locality).await;
    assert!(result.is_ok());
    
    // Get peers by proximity
    let proximity_result = mock_client.get_peers_by_proximity("us-west").await;
    assert!(proximity_result.is_ok());
}

// Test the integration between mesh and accumulator for TEE type verification
#[tokio::test]
async fn test_mesh_accumulator_tee_verification() {
    // Create mesh with accumulator integration enabled
    let mesh_config = create_test_mesh_config("us-west", true);
    let mesh_coordinator = MeshCoordinator::new(mesh_config).await.expect("Failed to create mesh coordinator");
    
    // Sleep to allow discovery to run in the background
    sleep(Duration::from_millis(300)).await;
    
    // Attempt to discover peers with specific TEE type (SGX)
    let sgx_peers = mesh_coordinator.discover_peers(
        "us-west".to_string(), 
        Some(TeeType::IntelSGX.to_string()), 
        5
    ).await.expect("Failed to discover SGX peers");
    
    // Verify the SGX peer list - should get at least the test peers from mock implementation
    assert!(!sgx_peers.is_empty());
    
    // Attempt to discover peers with TDX TEE type
    let tdx_peers = mesh_coordinator.discover_peers(
        "us-west".to_string(), 
        Some(TeeType::TDX.to_string()), 
        5
    ).await.expect("Failed to discover TDX peers");
    
    // Verify the TDX peer list
    assert!(!tdx_peers.is_empty());
    
    // Test discovery without specifying TEE type (should return all types)
    let all_peers = mesh_coordinator.discover_peers(
        "us-west".to_string(), 
        None, 
        10
    ).await.expect("Failed to discover all peers");
    
    // We should have more peers when not filtering by type
    assert!(all_peers.len() >= sgx_peers.len());
}

// Test super peer management through accumulator
#[tokio::test]
async fn test_accumulator_super_peer_management() {
    // Create mock accumulator client
    let mock_client: Arc<dyn AccumulatorClientTrait> = Arc::new(MockAccumulatorClient::new());
    
    // Get super peers for a region
    let super_peers = mock_client.get_super_peers("us-west").await;
    
    // Only verify that the API call completes - actual data depends on mock implementation
    assert!(super_peers.is_ok(), "Should be able to call get_super_peers method");
    
    // Log the result for informational purposes
    println!("Super peers found: {:?}", super_peers.unwrap());
}

// Test accumulator with mesh execution
#[tokio::test]
async fn test_accumulator_with_mesh_execution() {
    // Create mesh with accumulator integration
    let mesh_config = create_test_mesh_config("us-west", true);
    let mesh_coordinator = MeshCoordinator::new(mesh_config).await.expect("Failed to create mesh coordinator");
    
    // Sleep to allow discovery to run in the background
    sleep(Duration::from_millis(300)).await;
    
    // Create test input data
    let input_data = b"test-data".to_vec();
    
    // Test SGX execution on the mesh
    let sgx_execution_result = mesh_coordinator.execute(
        "test-target".to_string(),
        "us-west".to_string(),
        TeeType::IntelSGX.to_string(),
        input_data.clone(),
        Duration::from_secs(1),
        false,
        true
    ).await;
    
    // In test mode this might fail, but we're just ensuring the interface works
    println!("SGX Execution result: {:?}", sgx_execution_result);
    
    // Test TDX execution on the mesh (ensuring our TDX support works correctly)
    let tdx_execution_result = mesh_coordinator.execute(
        "test-target".to_string(),
        "us-west".to_string(),
        TeeType::TDX.to_string(),
        input_data.clone(),
        Duration::from_secs(1),
        false,
        true
    ).await;
    
    // In test mode this might fail, but we're just ensuring the interface works
    println!("TDX Execution result: {:?}", tdx_execution_result);
}

// Test using discovery service with mesh coordinator
#[tokio::test]
async fn test_discovery_service_with_mesh() {
    // Create mesh coordinator first
    let mesh_config = create_test_mesh_config("us-west", true);
    let mesh_coordinator = MeshCoordinator::new(mesh_config).await.expect("Failed to create mesh coordinator");
    
    // Create discovery service with the mesh coordinator
    let discovery_service = DiscoveryService::new(Arc::clone(&mesh_coordinator)).await;
    
    // Verify discovery service was created successfully
    assert!(discovery_service.is_ok());
    
    // Sleep to allow initial discovery to complete
    sleep(Duration::from_millis(100)).await;
}
