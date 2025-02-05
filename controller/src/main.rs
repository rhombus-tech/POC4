use std::sync::Arc;
use tokio::sync::RwLock;
use clap::Parser;
use tonic::transport::Server;

use tee_interface::prelude::*;
use crate::enarx::EnarxController;
use crate::server::TeeExecutionService;

mod enarx;
mod server;
mod verification;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 50051)]
    port: u16,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(if args.debug {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .init();

    // Create controllers for each TEE type
    let sgx_controller = Arc::new(RwLock::new(EnarxController::new(
        TeeType::SGX,
        "sgx_config.json".to_string(),
    )));

    let sev_controller = Arc::new(RwLock::new(EnarxController::new(
        TeeType::SEV,
        "sev_config.json".to_string(),
    )));

    // Create service
    let service = TeeExecutionService::new(sgx_controller.clone(), sev_controller.clone());

    // Start server
    let addr = format!("[::1]:{}", args.port).parse()?;
    println!("Starting TEE execution service on {}", addr);

    Server::builder()
        .add_service(server::tee::tee_execution_server::TeeExecutionServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_server_startup() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let sgx_controller = Arc::new(RwLock::new(EnarxController::new(
                TeeType::SGX,
                "test_sgx_config.json".to_string(),
            )));

            let sev_controller = Arc::new(RwLock::new(EnarxController::new(
                TeeType::SEV,
                "test_sev_config.json".to_string(),
            )));

            let service = TeeExecutionService::new(sgx_controller, sev_controller);
            let addr = "[::1]:50052".parse().unwrap();

            let server = Server::builder()
                .add_service(server::tee::tee_execution_server::TeeExecutionServer::new(service))
                .serve(addr);

            tokio::time::timeout(std::time::Duration::from_secs(1), server)
                .await
                .expect_err("Server should not complete");
        });
    }

    #[test]
    fn test_execution_result() {
        let result = ExecutionResult {
            result: b"test output".to_vec(),
            attestation: TeeAttestation {
                enclave_id: [1u8; 32],
                measurement: vec![2u8; 32],
                data: b"test".to_vec(),
                signature: vec![3u8; 64],
                region_proof: Some(vec![4u8; 32]),
            },
            state_hash: vec![5u8; 32],
            stats: ExecutionStats {
                execution_time: 1000,
                memory_used: 1024,
                syscall_count: 10,
            },
        };

        assert!(!result.result.is_empty());
        assert!(!result.attestation.measurement.is_empty());
        assert!(!result.state_hash.is_empty());
    }
}