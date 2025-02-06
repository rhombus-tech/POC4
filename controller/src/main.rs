use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Server;
use clap::Parser;
use crate::server::{TeeServer, TeeExecutionWrapper};
use crate::proto::teeservice::tee_execution_server::TeeExecutionServer;
use crate::simulator::SimulatorController;

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

    // Create simulator executor
    let simulator = SimulatorController::new();
    
    // Create TEE server with simulator executor
    let service = TeeServer::new(Box::new(simulator));
    let service = Arc::new(RwLock::new(service));
    let wrapper = TeeExecutionWrapper::new(service);
    
    // Start gRPC server
    let addr = args.addr.parse()?;
    println!("Starting TEE service on {}", addr);

    Server::builder()
        .add_service(TeeExecutionServer::new(wrapper))
        .serve(addr)
        .await?;

    Ok(())
}