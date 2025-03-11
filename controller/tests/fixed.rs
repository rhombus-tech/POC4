// Helper functions for testing
/// Creates a mock TEE executor for testing
fn create_mock_executor() -> Arc<dyn TeeExecutor> {
    Arc::new(MockTeeExecutor::new())
}

/// Creates a test controller with the given executor
fn create_test_controller(executor: Arc<dyn TeeExecutor>) -> Arc<HyperTeeController> {
    Arc::new(HyperTeeController::new(executor))
}
