use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::time::timeout;
use tee_controller::tee_peer::TeePeerService;
use tee_controller::proto::teeservice;
use tonic::Request;
use chrono;

// This test simulates a regional mesh network with two TEE peers
#[tokio::test(flavor = "multi_thread")]
async fn test_regional_mesh_network() {
    // Increase timeout to 20 seconds and add more debugging
    if let Err(_) = timeout(Duration::from_secs(20), async {
        println!("Starting regional mesh network test...");
        
        // Setup for primary TEE
        let (primary_tx, mut primary_rx) = mpsc::channel::<(teeservice::ExecutionRequest, mpsc::Sender<Result<teeservice::ExecutionResult, tonic::Status>>)>(32);
        let primary_service = TeePeerService::new(
            "primary-tee".to_string(),
            "region-1".to_string(),
            primary_tx,
        );
        let primary_addr = SocketAddr::from_str("127.0.0.1:50081").unwrap();
        
        // Setup for secondary TEE
        let (secondary_tx, mut secondary_rx) = mpsc::channel::<(teeservice::ExecutionRequest, mpsc::Sender<Result<teeservice::ExecutionResult, tonic::Status>>)>(32);
        let secondary_service = TeePeerService::new(
            "secondary-tee".to_string(),
            "region-1".to_string(),
            secondary_tx,
        );
        let secondary_addr = SocketAddr::from_str("127.0.0.1:50082").unwrap();
        
        // Spawn a task to handle requests received by the primary service
        tokio::spawn(async move {
            println!("Primary service handler task started");
            while let Some((request, response_tx)) = primary_rx.recv().await {
                println!("Primary service received request: {:?}", request);
                
                // Mock successful response for any request
                let result = teeservice::ExecutionResult {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    attestations: vec![],
                    state_hash: vec![1, 2, 3],
                    result: b"success".to_vec(),
                    execution_time: 5,
                    memory_used: 1024,
                    syscall_count: 10,
                };
                
                if let Err(e) = response_tx.send(Ok(result)).await {
                    println!("Error sending response: {:?}", e);
                }
            }
        });
        
        // Spawn a task to handle requests received by the secondary service
        tokio::spawn(async move {
            println!("Secondary service handler task started");
            while let Some((request, response_tx)) = secondary_rx.recv().await {
                println!("Secondary service received request: {:?}", request);
                
                // Mock successful response for any request
                let result = teeservice::ExecutionResult {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    attestations: vec![],
                    state_hash: vec![1, 2, 3],
                    result: b"success".to_vec(),
                    execution_time: 5,
                    memory_used: 1024,
                    syscall_count: 10,
                };
                
                if let Err(e) = response_tx.send(Ok(result)).await {
                    println!("Error sending response: {:?}", e);
                }
            }
        });
        
        // Start primary service
        let primary_service_handle = tokio::spawn(async move {
            println!("Starting primary service on {}", primary_addr);
            if let Err(e) = primary_service.start(primary_addr).await {
                println!("Primary service error: {:?}", e);
            }
            println!("Primary service exited");
        });
        
        // Start secondary service
        let secondary_service_handle = tokio::spawn(async move {
            println!("Starting secondary service on {}", secondary_addr);
            if let Err(e) = secondary_service.start(secondary_addr).await {
                println!("Secondary service error: {:?}", e);
            }
            println!("Secondary service exited");
        });
        
        // Wait for services to start
        println!("Waiting for services to start...");
        sleep(Duration::from_millis(1000)).await;
        
        // Connect to primary service as a client
        println!("Connecting to primary service...");
        let primary_channel = match tonic::transport::Channel::from_static("http://127.0.0.1:50081")
            .connect()
            .await {
                Ok(channel) => {
                    println!("Successfully connected to primary service");
                    channel
                },
                Err(e) => {
                    println!("Error connecting to primary service: {:?}", e);
                    panic!("Failed to connect to primary service: {:?}", e);
                }
            };
        
        let mut primary_client = teeservice::tee_execution_client::TeeExecutionClient::new(primary_channel);
        
        // Test get_regions functionality
        let get_regions_request = teeservice::GetRegionsRequest {};
        let get_regions_response = primary_client.get_regions(Request::new(get_regions_request)).await;
        
        assert!(get_regions_response.is_ok(), "Get regions request failed");
        let regions = get_regions_response.unwrap().into_inner().regions;
        println!("Discovered regions: {:?}", regions);
        
        // Test get_attestations functionality
        let get_attestations_request = teeservice::GetAttestationsRequest {
            region_id: "region-1".to_string(),
        };
        
        let get_attestations_response = primary_client.get_attestations(Request::new(get_attestations_request)).await;
        assert!(get_attestations_response.is_ok(), "Get attestations request failed");
        
        let attestations = get_attestations_response.unwrap().into_inner().attestations;
        println!("Received attestations: {:?}", attestations);
        
        // Test execute functionality
        let execution_request = teeservice::ExecutionRequest {
            id_to: "test-contract".to_string(),
            function_call: "test_function".to_string(),
            parameters: b"test_parameters".to_vec(),
            region_id: "region-1".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        };
        
        let execution_response = primary_client.execute(Request::new(execution_request)).await;
        println!("Primary error response: {:?}", execution_response.as_ref().err());
        assert!(execution_response.is_ok(), "Execution request failed");
        
        println!("Execution response: {:?}", execution_response.unwrap().into_inner());
        
        // Connect to secondary service as a client
        println!("Connecting to secondary service...");
        let secondary_channel = match tonic::transport::Channel::from_static("http://127.0.0.1:50082")
            .connect()
            .await {
                Ok(channel) => {
                    println!("Successfully connected to secondary service");
                    channel
                },
                Err(e) => {
                    println!("Error connecting to secondary service: {:?}", e);
                    panic!("Failed to connect to secondary service: {:?}", e);
                }
            };
        
        let mut secondary_client = teeservice::tee_execution_client::TeeExecutionClient::new(secondary_channel);
        
        // Test get_regions functionality
        let get_regions_request = teeservice::GetRegionsRequest {};
        let get_regions_response = secondary_client.get_regions(Request::new(get_regions_request)).await;
        
        assert!(get_regions_response.is_ok(), "Get regions request from secondary failed");
        let regions = get_regions_response.unwrap().into_inner().regions;
        println!("Discovered regions from secondary: {:?}", regions);
        
        // Test deploy_contract functionality
        let deploy_request = teeservice::DeployContractRequest {
            contract_bytes: b"test_wasm_code".to_vec(),
            region_id: "region-1".to_string(),
        };
        
        let deploy_response = secondary_client.deploy_contract(Request::new(deploy_request)).await;
        println!("Secondary deploy error response: {:?}", deploy_response.as_ref().err());
        assert!(deploy_response.is_ok(), "Deploy contract request failed");
        
        println!("Deploy contract response: {:?}", deploy_response.unwrap().into_inner());
        
        // Clean up
        println!("Test completed successfully, cleaning up...");
        primary_service_handle.abort();
        secondary_service_handle.abort();
    }).await {
        panic!("Test timed out");
    }
}
