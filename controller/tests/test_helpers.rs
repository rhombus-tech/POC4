use std::sync::Arc;
use tee_controller::HyperTeeController;
use tee_interface::TeeExecutor;
use super::MockTeeExecutor;

/// Creates a mock TEE executor for testing
pub fn create_mock_executor() -> Arc<MockTeeExecutor> {
    Arc::new(MockTeeExecutor::new())
}

/// Creates a test controller
pub async fn create_test_controller() -> Arc<HyperTeeController> {
    Arc::new(HyperTeeController::new().await)
}

// TODO: Let's look at the test helpers to understand how MockTeeExecutor is defined
