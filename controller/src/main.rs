use tee_interface::prelude::*;
use clap::{Parser, Subcommand};
use tokio;
use sha2::{Sha256, Digest};

mod enarx;
mod verification;

use enarx::EnarxController;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute code in TEE
    Execute {
        /// Path to WASM file
        #[arg(short, long)]
        wasm_file: String,
        
        /// Input data
        #[arg(short, long)]
        input: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut sgx_controller = EnarxController::new(TeeType::Sgx);
    let mut sev_controller = EnarxController::new(TeeType::Sev);

    // Initialize both controllers
    sgx_controller.init().await?;
    sev_controller.init().await?;

    match &cli.command {
        Commands::Execute { wasm_file, input } => {
            // Read WASM file
            let wasm_bytes = tokio::fs::read(wasm_file)
                .await
                .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to read WASM file: {}", e))))?;

            // Create execution payload
            let mut hasher = Sha256::new();
            hasher.update(&wasm_bytes);
            let hash: [u8; 32] = hasher.finalize().into();

            let payload = ExecutionPayload {
                execution_id: 1,
                input: input.as_ref().map(|s| s.as_bytes().to_vec()).unwrap_or_default(),
                params: ExecutionParams {
                    expected_hash: Some(hash),
                    detailed_proof: true,
                },
            };

            // Execute on both platforms
            let sgx_result = sgx_controller.execute(&payload).await?;
            let sev_result = sev_controller.execute(&payload).await?;

            println!("SGX Result: {:?}", sgx_result);
            println!("SEV Result: {:?}", sev_result);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_paired_execution() {
        let mut sgx_controller = EnarxController::new(TeeType::Sgx);
        let mut sev_controller = EnarxController::new(TeeType::Sev);

        sgx_controller.init().await.unwrap();
        sev_controller.init().await.unwrap();

        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test input".to_vec(),
            params: ExecutionParams::default(),
        };

        let sgx_result = sgx_controller.execute(&payload).await.unwrap();
        let sev_result = sev_controller.execute(&payload).await.unwrap();

        assert_eq!(sgx_result.output, sev_result.output);
        assert_eq!(sgx_result.attestations[0].tee_type, TeeType::Sgx);
        assert_eq!(sev_result.attestations[0].tee_type, TeeType::Sev);
    }
}