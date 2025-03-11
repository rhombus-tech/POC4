pub mod proto;
pub mod server;
pub mod simulator;
pub mod enarx;
pub mod paired_executor;
pub mod hyper_integration;
pub mod coordinator_client;
pub mod tee_peer;
pub mod tee_peer_client;
pub mod metrics;
pub mod mesh;
pub mod policy;
pub mod integration_tests;
pub mod discovery_service;
pub mod discovery_integration;

pub use simulator::SimulatorController;
pub use enarx::EnarxController;
pub use paired_executor::TeeExecutorPair;
pub use hyper_integration::HyperTeeController;
pub use server::TeeServer;
pub use proto::teeservice::tee_execution_server::TeeExecutionServer;
pub use coordinator_client::CoordinatorClient;
pub use tee_peer::TeePeerService;
pub use tee_peer_client::TeeServiceClient;
pub use metrics::{MetricsStore, RoutingStrategy, TeePerformanceMetrics};
pub use mesh::{MeshCoordinator, MeshConfig};
pub use policy::{Policy, PolicyManager, SharedPolicyManager, PolicyRule, CircuitBreaker, CircuitBreakerLevel};
pub use discovery_service::{DiscoveryService, DiscoveryServiceConfig};
pub use discovery_integration::{EnhancedDiscoveryIntegration, EnhancedDiscoveryConfig};

#[cfg(test)]
mod policy_test;
