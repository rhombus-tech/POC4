use clap::Parser;
use tee_interface::prelude::*;
use std::path::PathBuf;

mod enarx;
mod verification;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to WASM module
    #[arg(short, long)]
    wasm: PathBuf,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Check platform support
    let (sgx_supported, sev_supported) = enarx::verify_platforms()?;
    println!("Platform support:");
    println!("  SGX: {}", sgx_supported);
    println!("  SEV: {}", sev_supported);

    if !sgx_supported && !sev_supported {
        eprintln!("No supported TEE platforms found");
        std::process::exit(1);
    }

    // Execute in both TEEs
    let sgx_result = enarx::execute_sgx(&args.wasm).await?;
    let sev_result = enarx::execute_sev(&args.wasm).await?;

    // Print attestations if verbose
    if args.verbose {
        println!("\nSGX Attestation:");
        print_attestation(&sgx_result.attestations[0]);
        println!("\nSEV Attestation:");
        print_attestation(&sev_result.attestations[0]);
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

    #[tokio::test]
    async fn test_basic_execution() {
        // Create test WASM file
        let wasm_path = PathBuf::from("test.wasm");
        fs::write(&wasm_path, b"test wasm").unwrap();

        // Create test args
        let args = Args {
            wasm: wasm_path.clone(),
            verbose: false,
        };

        // Test execution
        let result = enarx::execute_sgx(&args.wasm).await;
        assert!(result.is_err()); // Should fail because we provided invalid WASM

        // Cleanup
        fs::remove_file(wasm_path).unwrap();
    }
}