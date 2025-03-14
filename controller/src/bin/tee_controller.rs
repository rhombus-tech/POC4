use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use clap::{Parser, Subcommand};
use log::{info, error, debug};
use tee_controller::enarx::controller::EnarxController;
use tee_interface::TeeType as InterfaceTeeType;
use tee_controller::mesh::{MeshConfig, MeshCoordinator, TeeType as MeshTeeType};
use tee_controller::paired_executor::TeeExecutorPair;
use tee_controller::server::TeeServer;
use tee_controller::MeshExecutionExtension;
use std::time::Duration;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Option<Commands>,

    /// Port to listen on
    #[clap(short, long, default_value = "50051")]
    port: u16,
    
    /// Base directory for storing contracts and state
    #[clap(short, long, default_value = "/tmp/tee-controller")]
    base_dir: PathBuf,
    
    /// Use simulation mode instead of real TEE
    #[clap(short, long)]
    simulate: bool,

    /// Enable mesh networking
    #[clap(long)]
    mesh_enabled: bool,

    /// Region ID for this TEE controller
    #[clap(long, default_value = "default-region")]
    region_id: String,

    /// TEE ID for this controller (auto-generated if not specified)
    #[clap(long)]
    tee_id: Option<String>,

    /// Preferred TEE type for execution (sgx or sev)
    #[clap(long, default_value = "sgx")]
    tee_type: String,

    /// Execution mode for HyperTeeController (auto, mesh, coordinator, direct)
    #[clap(long, default_value = "auto")]
    execution_mode: String,

    /// Peer discovery endpoint
    #[clap(long, default_value = "localhost:50052")]
    discovery_endpoint: String,

    /// Maximum number of peers to track per region
    #[clap(long, default_value = "10")]
    max_peers: usize,

    /// Circuit breaker threshold (in ms) - fallback to coordinator if latency exceeds this value
    #[clap(long, default_value = "100")]
    circuit_breaker_threshold_ms: u64,

    /// Peer refresh interval (in seconds)
    #[clap(long, default_value = "60")]
    peer_refresh_interval_sec: u64,

    /// Wasm module path for direct execution
    #[clap(long)]
    wasm_module: Option<PathBuf>,

    /// Input file for direct execution
    #[clap(long)]
    input: Option<PathBuf>,

    /// Backend for direct execution (sgx, sev)
    #[clap(long, default_value = "sgx")]
    backend: String,

    /// Verbose output
    #[clap(long)]
    verbose: bool,

    /// Accumulator endpoint
    #[clap(long)]
    accumulator_endpoint: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute using the mesh network
    MeshExecute {
        /// Target TEE ID
        #[clap(long)]
        target_tee: String,
        
        /// Region for execution
        #[clap(long)]
        region: String,
        
        /// TEE type (sgx, sev)
        #[clap(long)]
        tee_type: String,
        
        /// Timeout in milliseconds
        #[clap(long, default_value = "5000")]
        timeout: u64,
        
        /// Execute asynchronously
        #[clap(long)]
        r#async: bool,
        
        /// Allow fallback to alternate execution paths
        #[clap(long)]
        allow_fallback: bool,
        
        /// WebAssembly module to execute
        #[clap(long)]
        wasm_module: PathBuf,
        
        /// Input file
        #[clap(long)]
        input: PathBuf,
    },
    
    /// Discover peers in the mesh
    DiscoverPeers {
        /// Region to search in
        #[clap(long)]
        region: String,
        
        /// Filter by TEE type (sgx, sev)
        #[clap(long)]
        tee_type: Option<String>,
        
        /// Maximum number of results
        #[clap(long, default_value = "10")]
        max_results: usize,
    },
    
    /// Synchronize state with another TEE
    SyncState {
        /// Object ID to synchronize
        #[clap(long)]
        object_id: String,
        
        /// Target TEE ID
        #[clap(long)]
        target_tee: String,
        
        /// Use delta synchronization
        #[clap(long)]
        use_deltas: bool,
    },
    
    /// Execute with mesh cache
    ExecuteWithMeshCache {
        /// Target TEE ID
        #[clap(long)]
        target_tee: String,
        
        /// Region for execution
        #[clap(long)]
        region: String,
        
        /// TEE type (sgx, sev)
        #[clap(long)]
        tee_type: String,
        
        /// Timeout in milliseconds
        #[clap(long, default_value = "5000")]
        timeout: u64,
        
        /// Execute asynchronously
        #[clap(long)]
        r#async: bool,
        
        /// Allow fallback to alternate execution paths
        #[clap(long)]
        allow_fallback: bool,
        
        /// WebAssembly module to execute
        #[clap(long)]
        wasm_module: PathBuf,
        
        /// Input file
        #[clap(long)]
        input: PathBuf,
        
        /// Use cache
        #[clap(long)]
        use_cache: bool,
        
        /// Cache TTL in seconds
        #[clap(long, default_value = "300")]
        cache_ttl_sec: u64,
        
        /// Stale result timeout in milliseconds
        #[clap(long, default_value = "1000")]
        stale_result_timeout_ms: u64,
    },
    
    /// Execute in paired mode
    ExecutePaired {
        /// WebAssembly module to execute
        #[clap(long)]
        wasm_module: PathBuf,
        
        /// Input file
        #[clap(long)]
        input: PathBuf,
        
        /// Contract ID
        #[clap(long)]
        contract_id: String,
        
        /// Operation ID
        #[clap(long)]
        operation_id: String,
        
        /// Timeout in milliseconds
        #[clap(long, default_value = "5000")]
        timeout: u64,
        
        /// Use cache
        #[clap(long)]
        use_cache: bool,

        /// Function call (method name)
        #[clap(long)]
        function_call: Option<String>,
    },
    
    /// Verify available TEE platforms
    VerifyPlatforms,
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Initialize logging
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info")
    );
    
    // Parse command line arguments
    let args = Args::parse();
    
    if args.verbose {
        log::set_max_level(log::LevelFilter::Debug);
    }
    
    info!("Starting TEE Controller with base directory: {:?}", args.base_dir);
    
    // Set the execution mode environment variable based on CLI parameter
    std::env::set_var("TEE_EXECUTION_MODE", &args.execution_mode);
    info!("Setting execution mode to: {}", args.execution_mode);
    
    // Create the SGX and SEV TEE executors
    let sgx_controller = match EnarxController::new(
        InterfaceTeeType::SGX, 
        args.base_dir.join("sgx").to_str().unwrap_or("./sgx"), 
        args.simulate
    ).await {
        Ok(controller) => controller,
        Err(e) => {
            error!("Failed to initialize SGX controller: {:?}", e);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other, 
                format!("SGX initialization error: {:?}", e)
            ));
        }
    };
    
    let sev_controller = match EnarxController::new(
        InterfaceTeeType::SEV, 
        args.base_dir.join("sev").to_str().unwrap_or("./sev"), 
        args.simulate
    ).await {
        Ok(controller) => controller,
        Err(e) => {
            error!("Failed to initialize SEV controller: {:?}", e);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other, 
                format!("SEV initialization error: {:?}", e)
            ));
        }
    };
    
    // Wrap the controllers in Arc<RwLock>
    let sgx = Arc::new(RwLock::new(sgx_controller));
    let sev = Arc::new(RwLock::new(sev_controller));
    
    // Generate a TEE ID if not provided
    let tee_id = match args.tee_id {
        Some(id) => id,
        None => {
            use uuid::Uuid;
            format!("tee-{}", Uuid::new_v4())
        }
    };
    
    info!("Using TEE ID: {}", tee_id);
    
    // Setup mesh coordinator if enabled
    let mesh_config = if args.mesh_enabled {
        info!("Initializing mesh network support for region: {}", args.region_id);
        let config = MeshConfig {
            region_id: args.region_id.clone(),
            tee_id: tee_id.clone(),
            endpoint: format!("{}:{}", args.discovery_endpoint, args.port),
            discovery_endpoint: args.discovery_endpoint.clone(),
            max_peers: args.max_peers,
            discovery_interval_sec: args.peer_refresh_interval_sec,
            circuit_breaker_threshold: Duration::from_millis(args.circuit_breaker_threshold_ms),
            peer_refresh_interval: Duration::from_secs(args.peer_refresh_interval_sec),
            enhanced_discovery: false, // Using the traditional discovery service by default
            discovery_config: None, // Can be configured via command line args in the future
            accumulator_endpoint: args.accumulator_endpoint.clone().unwrap_or_else(|| "http://localhost:8090".to_string()).into(),
            local_identity: tee_id.clone().into(), // Using tee_id as local identity
        };
        Some(config)
    } else {
        debug!("Mesh networking is disabled");
        None
    };
    
    let mesh_coordinator = match mesh_config {
        Some(config) => {
            match MeshCoordinator::new(config).await {
                Ok(coordinator) => Some(coordinator),
                Err(e) => {
                    error!("Failed to initialize mesh coordinator: {:?}", e);
                    None
                }
            }
        }
        None => None,
    };
    
    // Create the paired executor
    let paired_executor = TeeExecutorPair::new(sgx, sev, mesh_coordinator);
    let executor = Arc::new(paired_executor);
    
    // Process subcommands if present
    if let Some(cmd) = args.command {
        match cmd {
            Commands::MeshExecute { 
                target_tee, 
                region, 
                tee_type, 
                timeout, 
                r#async, 
                allow_fallback, 
                wasm_module, 
                input 
            } => {
                return handle_mesh_execute(
                    &executor,
                    target_tee,
                    region,
                    tee_type,
                    Duration::from_millis(timeout),
                    r#async,
                    allow_fallback,
                    wasm_module,
                    input,
                ).await;
            },
            Commands::DiscoverPeers { 
                region, 
                tee_type, 
                max_results 
            } => {
                return handle_discover_peers(
                    &executor,
                    region,
                    tee_type,
                    max_results,
                ).await;
            },
            Commands::SyncState { 
                object_id, 
                target_tee, 
                use_deltas 
            } => {
                return handle_sync_state(
                    &executor,
                    object_id,
                    target_tee,
                    use_deltas,
                ).await;
            },
            Commands::ExecuteWithMeshCache {
                target_tee,
                region,
                tee_type,
                timeout,
                r#async,
                allow_fallback,
                wasm_module,
                input,
                use_cache,
                cache_ttl_sec,
                stale_result_timeout_ms,
            } => {
                return handle_execute_with_mesh_cache(
                    &executor,
                    target_tee,
                    region,
                    tee_type,
                    Duration::from_millis(timeout),
                    r#async,
                    allow_fallback,
                    wasm_module,
                    input,
                    use_cache,
                    Duration::from_secs(cache_ttl_sec),
                    Duration::from_millis(stale_result_timeout_ms),
                ).await;
            },
            Commands::ExecutePaired {
                wasm_module,
                input,
                contract_id,
                operation_id,
                timeout,
                use_cache,
                function_call,
            } => {
                return handle_execute_paired(
                    &executor,
                    wasm_module,
                    input,
                    contract_id,
                    operation_id,
                    Duration::from_millis(timeout),
                    use_cache,
                    function_call,
                ).await;
            }
            Commands::VerifyPlatforms => {
                return handle_verify_platforms(&executor).await;
            }
        }
    }
    
    // If we reach here, no subcommand was specified, so start the server
    // Create the TEE server
    let server = TeeServer::new(executor);
    
    // Start the server
    info!("Starting TEE server on port {}", args.port);
    server.serve(args.port).await.map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("Server error: {}", e))
    })?;
    
    Ok(())
}

async fn handle_mesh_execute(
    executor: &Arc<TeeExecutorPair>,
    target_tee: String,
    region: String,
    tee_type: String,
    timeout: Duration,
    is_async: bool,
    allow_fallback: bool,
    wasm_module: PathBuf,
    input_file: PathBuf,
) -> Result<(), std::io::Error> {
    info!("Executing via mesh network: tee={}, region={}", target_tee, region);
    
    // Read input file
    let input = match tokio::fs::read(&input_file).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read input file: {:?}", e);
            return Err(e);
        }
    };
    
    // Execute via mesh
    let tee_type = match tee_type.to_lowercase().as_str() {
        "sgx" => MeshTeeType::IntelSGX,
        "sev" => MeshTeeType::SEV,
        _ => {
            error!("Unsupported TEE type: {}", tee_type);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Unsupported TEE type: {}", tee_type)
            ));
        }
    };
    
    let result = executor.execute_mesh(
        target_tee,
        region,
        tee_type,
        input,
        timeout,
        is_async,
        allow_fallback,
    ).await;
    
    match result {
        Ok(result) => {
            // Output result as JSON
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        },
        Err(e) => {
            error!("Mesh execution failed: {:?}", e);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Mesh execution error: {:?}", e)
            ))
        }
    }
}

async fn handle_discover_peers(
    executor: &Arc<TeeExecutorPair>,
    region: String,
    tee_type: Option<String>,
    max_results: usize,
) -> Result<(), std::io::Error> {
    info!("Discovering peers in region: {}", region);
    
    // Convert tee_type string to TeeType enum if provided
    let tee_type_enum = match tee_type {
        Some(typ) => {
            match typ.to_lowercase().as_str() {
                "sgx" => Some(MeshTeeType::IntelSGX),
                "sev" => Some(MeshTeeType::SEV),
                _ => {
                    error!("Unsupported TEE type: {}", typ);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Unsupported TEE type: {}", typ)
                    ));
                }
            }
        },
        None => None,
    };
    
    let result = executor.discover_peers(region, tee_type_enum, max_results).await;
    
    match result {
        Ok(peers) => {
            // Output peers as JSON
            println!("{}", serde_json::to_string_pretty(&peers).unwrap());
            Ok(())
        },
        Err(e) => {
            error!("Peer discovery failed: {:?}", e);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Peer discovery error: {:?}", e)
            ))
        }
    }
}

async fn handle_sync_state(
    executor: &Arc<TeeExecutorPair>,
    object_id: String,
    target_tee: String,
    use_deltas: bool,
) -> Result<(), std::io::Error> {
    info!("Synchronizing state with TEE: {}", target_tee);
    
    let result = executor.sync_state(object_id, target_tee, use_deltas).await;
    
    match result {
        Ok(sync_result) => {
            // Output sync result as JSON
            println!("{}", serde_json::to_string_pretty(&sync_result).unwrap());
            Ok(())
        },
        Err(e) => {
            error!("State synchronization failed: {:?}", e);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("State synchronization error: {:?}", e)
            ))
        }
    }
}

async fn handle_execute_with_mesh_cache(
    executor: &Arc<TeeExecutorPair>,
    target_tee: String,
    region: String,
    tee_type: String,
    timeout: Duration,
    is_async: bool,
    allow_fallback: bool,
    wasm_module: PathBuf,
    input_file: PathBuf,
    use_cache: bool,
    cache_ttl: Duration,
    stale_result_timeout: Duration,
) -> Result<(), std::io::Error> {
    info!("Executing with mesh cache: tee={}, region={}", target_tee, region);
    
    // Read input file
    let input = match tokio::fs::read(&input_file).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read input file: {:?}", e);
            return Err(e);
        }
    };
    
    // Execute via mesh
    let tee_type = match tee_type.to_lowercase().as_str() {
        "sgx" => MeshTeeType::IntelSGX,
        "sev" => MeshTeeType::SEV,
        _ => {
            error!("Unsupported TEE type: {}", tee_type);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Unsupported TEE type: {}", tee_type)
            ));
        }
    };
    
    let result = executor.execute_with_mesh_cache(
        target_tee,
        region,
        tee_type,
        input,
        timeout,
        is_async,
        allow_fallback,
        use_cache,
        cache_ttl,
        stale_result_timeout,
    ).await;
    
    match result {
        Ok(result) => {
            // Output result as JSON
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        },
        Err(e) => {
            error!("Mesh execution with cache failed: {:?}", e);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Mesh execution with cache error: {:?}", e)
            ))
        }
    }
}

async fn handle_execute_paired(
    executor: &Arc<TeeExecutorPair>,
    wasm_module: PathBuf,
    input_file: PathBuf,
    contract_id: String,
    operation_id: String,
    timeout: Duration,
    use_cache: bool,
    function_call: Option<String>,
) -> Result<(), std::io::Error> {
    info!("Executing in paired mode");
    
    // Read input file
    let input = match tokio::fs::read(&input_file).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read input file: {:?}", e);
            return Err(e);
        }
    };
    
    // Get the execution mode from environment if not set in CLI
    let execution_mode = std::env::var("TEE_EXECUTION_MODE").unwrap_or_else(|_| "auto".to_string());
    
    let result = match execution_mode.to_lowercase().as_str() {
        "mesh" => {
            info!("Using mesh execution path");
            // Create an execution payload with mesh-specific fields
            let payload = tee_interface::ExecutionPayload {
                input: input.clone(),
                params: tee_interface::types::ExecutionParams {
                    id_to: contract_id.clone(),
                    function_call: function_call.clone().unwrap_or_else(|| "execute".to_string()),
                    ..Default::default()
                },
                operation_id: Some(operation_id.clone()),
                // Use mesh-specific fields
                target_tee: None, // Will be automatically selected based on routing
                region_id: None,  // Will use the default region from TeeExecutorPair
                tee_type: None,   // Will use the default TEE type
                allow_fallback: Some(true), // Allow fallback to coordinator if mesh fails
                ..Default::default()
            };
            
            // Use the MeshExecutionExtension trait to attempt mesh execution first
            executor.try_mesh_execution(&payload).await.map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("Mesh execution failed: {:?}", e))
            })?
        },
        "coordinator" => {
            info!("Using coordinator-mediated execution path");
            executor.execute_paired(
                wasm_module,
                input,
                contract_id,
                operation_id,
                timeout,
                use_cache,
                function_call,
            ).await.map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("Coordinator execution failed: {:?}", e))
            })?
        },
        // "auto" or any other value
        _ => {
            info!("Using automatic execution path selection (mesh with coordinator fallback)");
            // Create an execution payload with mesh-specific fields
            let payload = tee_interface::ExecutionPayload {
                input: input.clone(),
                params: tee_interface::types::ExecutionParams {
                    id_to: contract_id.clone(),
                    function_call: function_call.clone().unwrap_or_else(|| "execute".to_string()),
                    ..Default::default()
                },
                operation_id: Some(operation_id.clone()),
                // Settings for automatic path selection
                allow_fallback: Some(true), // Allow fallback to coordinator if mesh fails
                ..Default::default()
            };
            
            // Try mesh execution first, fall back to coordinator if needed
            match executor.try_mesh_execution(&payload).await {
                Ok(Some(result)) => {
                    info!("Mesh execution succeeded");
                    result
                },
                Ok(None) => {
                    info!("Falling back to coordinator execution");
                    executor.execute_paired(
                        wasm_module,
                        input,
                        contract_id,
                        operation_id,
                        timeout,
                        use_cache,
                        function_call,
                    ).await.map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::Other, format!("Fallback execution failed: {:?}", e))
                    })?
                },
                Err(e) => {
                    error!("Mesh execution failed without fallback: {:?}", e);
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Execution failed: {:?}", e)));
                }
            }
        }
    };
    
    // Output result as JSON
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    Ok(())
}

async fn handle_verify_platforms(
    executor: &Arc<TeeExecutorPair>,
) -> Result<(), std::io::Error> {
    info!("Verifying available TEE platforms");
    
    let (sgx_available, sev_available) = executor.verify_platforms().await;
    
    // Output result as JSON
    let result = serde_json::json!({
        "sgx": sgx_available,
        "sev": sev_available,
    });
    
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    Ok(())
}
