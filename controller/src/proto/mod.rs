pub mod conversions;

// Move the inclusion of teeservice.rs to a separate file to avoid macro issues
pub mod teeservice {
    // Include the generated protobuf code
    include!("teeservice.rs");
    
    // Re-exports of key types for easier use
    pub use tee_execution_server::{TeeExecution, TeeExecutionServer};
    pub use tee_execution_client::TeeExecutionClient;
}
