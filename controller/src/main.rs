use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Server;
use clap::Parser;
use crate::server::TeeExecutionService;
use crate::server::teeservice::tee_execution_server::TeeExecutionServer;
use crate::server::TeeServiceWrapper;
use tee_interface::TeeController;

mod server;
mod simulator;
mod hyper_integration;
mod enarx;
mod paired_executor;
mod proto;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Region ID for TEE execution
    #[arg(short, long, default_value = "default")]
    region: String,
    #[arg(short, long, default_value = "0.0.0.0:50051")]
    addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let args = Args::parse();

    // Create and initialize service
    let mut service = TeeExecutionService::new();
    service.add_region(
        "default".to_string(),
        "sgx_config".to_string(),
        "sev_config".to_string(),
    );
    service.add_region(args.region.clone(), "sgx_config.json".into(), "sev_config.json".into());
    service.init().await?;
    let service = Arc::new(RwLock::new(service));
    
    // Create service wrapper and start gRPC server
    let wrapper = TeeServiceWrapper::new(service);
    let addr = args.addr.parse()?;
    println!("Starting TEE service on {}", addr);

    Server::builder()
        .add_service(TeeExecutionServer::new(wrapper))
        .serve(addr)
        .await?;

    Ok(())
}