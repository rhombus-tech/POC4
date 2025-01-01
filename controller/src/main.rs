use clap::Parser;
use tee_interface::prelude::*;
use std::process::Command;
use std::path::PathBuf;

mod enarx;
mod verification;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to WASM module
    #[arg(short, long)]
    wasm_module: PathBuf,

    /// Input data for computation
    #[arg(short, long)]
    input: PathBuf,

    /// Output path for results
    #[arg(short, long)]
    output: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize controller
    let controller = enarx::Controller::new(args.verbose)?;

    println!("Starting dual TEE execution...");

    // Execute in SGX
    println!("Executing in SGX...");
    let sgx_result = controller.execute_sgx(&args.wasm_module, &args.input).await?;

    // Execute in SEV
    println!("Executing in SEV...");
    let sev_result = controller.execute_sev(&args.wasm_module, &args.input).await?;

    // Verify results match
    println!("Verifying results...");
    let verification_result = verification::verify_results(&sgx_result, &sev_result)?;

    if verification_result.verified {
        println!("✅ Execution verified! Results match between SGX and SEV");
        println!("Result hash: {:?}", verification_result.result_hash);

        // Save result to output file
        std::fs::write(&args.output, &sgx_result.result)?;
        println!("Results saved to: {}", args.output.display());

        // Print attestation info if verbose
        if args.verbose {
            println!("\nSGX Attestation:");
            print_attestation(&sgx_result.attestation);
            println!("\nSEV Attestation:");
            print_attestation(&sev_result.attestation);
        }
    } else {
        eprintln!("❌ Verification failed: Results don't match!");
        std::process::exit(1);
    }

    Ok(())
}

fn print_attestation(attestation: &AttestationReport) {
    println!("  Type: {:?}", attestation.enclave_type);
    println!("  Timestamp: {}", attestation.timestamp);
    println!("  Measurement: {:?}", attestation.measurement);
    if !attestation.platform_data.is_empty() {
        println!("  Platform data size: {}", attestation.platform_data.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_execution() {
        // Create test files
        let temp_dir = tempfile::tempdir().unwrap();
        let wasm_path = temp_dir.path().join("test.wasm");
        let input_path = temp_dir.path().join("input.dat");
        let output_path = temp_dir.path().join("output.dat");

        // Write test data
        std::fs::write(&input_path, b"test data").unwrap();

        // Create test args
        let args = Args {
            wasm_module: wasm_path,
            input: input_path,
            output: output_path,
            verbose: false,
        };

        // Test execution
        let controller = enarx::Controller::new(false).unwrap();
        assert!(controller.execute_sgx(&args.wasm_module, &args.input).await.is_ok());
    }
}