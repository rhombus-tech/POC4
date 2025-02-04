use clap::Parser;
use tee_interface::prelude::*;
use crate::enarx::{EnarxController, EnarxConfig, RegionConfig};
use metrics_exporter_prometheus::PrometheusBuilder;
use tracing_subscriber::EnvFilter;
use std::net::SocketAddr;
use std::error::Error as StdError;

pub mod enarx;

type Result<T> = std::result::Result<T, Box<dyn StdError>>;

use std::path::PathBuf;
use borsh::BorshSerialize;
use std::env;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to WASM module
    #[arg(short, long)]
    wasm: PathBuf,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Region ID for execution
    #[arg(short, long, default_value = "default")]
    region: String,

    /// Enarx API URL
    #[arg(long, default_value = "http://localhost:8000")]
    api_url: String,
}

fn create_default_config(api_url: &str) -> EnarxConfig {
    EnarxConfig {
        api_url: api_url.to_string(),
        auth_token: None,
        regions: vec![
            RegionConfig {
                id: "default".to_string(),
                endpoint: format!("{}/default", api_url),
            },
            RegionConfig {
                id: "us-east-1".to_string(),
                endpoint: format!("{}/us-east-1", api_url),
            },
        ],
        max_retries: 3,
        initial_retry_delay_ms: 100,
        max_retry_delay_ms: 5000,
        request_timeout_ms: env::var("ENARX_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30000),
        connect_timeout_ms: env::var("ENARX_CONNECT_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10000),
    }
}

async fn init_metrics() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_target(false)
        .init();

    // Initialize Prometheus metrics exporter
    let builder = PrometheusBuilder::new();
    let _handle = builder
        .with_http_listener("0.0.0.0:9000".parse::<SocketAddr>().unwrap())
        .install()
        .expect("failed to install Prometheus metrics exporter");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    init_metrics().await?;

    let args = Args::parse();

    // Create controller with config
    let config = create_default_config(&args.api_url);
    let controller = EnarxController::new(config)?;

    // Check platform support
    let healthy = controller.health_check().await
        .map_err(|e| Box::new(e) as Box<dyn StdError>)?;
    if !healthy {
        eprintln!("No supported TEE platforms found");
        std::process::exit(1);
    }
    println!("TEE platform check passed");

    // Read WASM file
    let wasm_bytes = std::fs::read(&args.wasm)
        .map_err(|e| Box::new(e) as Box<dyn StdError>)?;

    // Prepare execution input
    let input = ExecutionInput {
        wasm_bytes,
        function: "main".to_string(),
        args: vec![],
    };

    // Serialize input
    let input_bytes = input.try_to_vec()
        .map_err(|e| Box::new(e) as Box<dyn StdError>)?;

    // Execute with attestation
    let result = controller.execute(
        args.region,
        input_bytes,
        true // Always require attestation for now
    ).await
        .map_err(|e| Box::new(e) as Box<dyn StdError>)?;

    // Print attestations if verbose
    if args.verbose {
        println!("\nSGX Attestation:");
        print_attestation(&result.attestations[0]);
        println!("\nSEV Attestation:");
        print_attestation(&result.attestations[1]);
    }

    Ok(())
}

fn print_attestation(attestation: &TeeAttestation) {
    println!("  Type: {:?}", attestation.tee_type);
    println!("  Measurement: {:?}", attestation.measurement);
    println!("  Signature: {:?}", attestation.signature);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn create_test_config() -> EnarxConfig {
        EnarxConfig {
            api_url: "http://localhost:8000".to_string(),
            auth_token: None,
            regions: vec![
                RegionConfig {
                    id: "test".to_string(),
                    endpoint: "http://localhost:8001".to_string(),
                },
            ],
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            request_timeout_ms: 30000,
            connect_timeout_ms: 10000,
        }
    }

    #[tokio::test]
    async fn test_basic_execution() {
        // Create test WASM file
        let temp_dir = tempdir().unwrap();
        let wasm_path = temp_dir.path().join("test.wasm");
        fs::write(&wasm_path, b"test wasm").unwrap();

        // Create controller with test config
        let controller = EnarxController::new(create_test_config()).unwrap();

        // Create input
        let input = ExecutionInput {
            wasm_bytes: b"test wasm".to_vec(),
            function: "main".to_string(),
            args: vec![],
        };

        // Execute
        let input_bytes = input.try_to_vec().unwrap();
        let result = controller.execute(
            "test".to_string(),
            input_bytes,
            true,
        ).await.unwrap();

        assert!(!result.output.is_empty());
        assert_eq!(result.attestations.len(), 2);
        assert_eq!(result.attestations[0].tee_type, TeeType::Sgx);
        assert_eq!(result.attestations[1].tee_type, TeeType::Sev);
    }
}