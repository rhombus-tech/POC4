use std::env;
use tee_controller::enarx::test_utils::EnarxTester;
use log::{info, error, LevelFilter};
use env_logger;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();
    
    info!("Starting Enarx Test Tool");
    
    // Check if we should use simulation mode
    let use_simulation = env::var("ENARX_SIMULATION")
        .map(|val| val.to_lowercase() == "true" || val == "1")
        .unwrap_or(false);
    
    // Create tester
    let tester = if use_simulation {
        info!("Using simulation mode");
        EnarxTester::new_with_simulation()
    } else {
        info!("Using real Enarx mode");
        EnarxTester::new()
    };
    
    match tester.initialize() {
        Ok(_) => {
            info!("Enarx test environment initialized successfully");
            
            // Run test execution
            match tester.test_execute().await {
                Ok(_) => {
                    info!(" Enarx test execution successful!");
                }
                Err(e) => {
                    error!(" Enarx test execution failed: {}", e);
                    std::process::exit(1);
                }
            }
            
            // Cleanup
            if let Err(e) = tester.cleanup() {
                error!("Cleanup failed: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to initialize Enarx test environment: {}", e);
            std::process::exit(1);
        }
    }
}
