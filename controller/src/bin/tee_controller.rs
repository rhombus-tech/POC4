use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use clap::Parser;
use log::{info, error};
use tee_controller::enarx::controller::EnarxController;
use tee_interface::types::TeeType;
use tee_controller::paired_executor::TeeExecutorPair;
use tee_controller::server::TeeServer;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Port to listen on
    #[clap(short, long, default_value = "50051")]
    port: u16,
    
    /// Base directory for storing contracts and state
    #[clap(short, long, default_value = "/tmp/tee-controller")]
    base_dir: PathBuf,
    
    /// Use simulation mode instead of real TEE
    #[clap(short, long)]
    simulate: bool,
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Initialize logging
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info")
    );
    
    // Parse command line arguments
    let args = Args::parse();
    
    info!("Starting TEE Controller with base directory: {:?}", args.base_dir);
    
    // Create the SGX and SEV TEE executors
    let sgx_controller = match EnarxController::new(
        TeeType::SGX, 
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
        TeeType::SEV, 
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
    
    // Create the paired executor
    let paired_executor = TeeExecutorPair::new(sgx, sev, None);
    let executor = Arc::new(paired_executor);
    
    // Create the TEE server
    let server = TeeServer::new(executor);
    
    // Start the server
    info!("Starting TEE server on port {}", args.port);
    server.serve(args.port).await.map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("Server error: {}", e))
    })?;
    
    Ok(())
}
