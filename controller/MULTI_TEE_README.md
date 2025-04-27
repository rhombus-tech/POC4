# Multi-TEE Pair Execution System

This document explains how to use the multi-TEE pair functionality in the Aristo execution system. The system now supports coordinating multiple TEE pairs to execute contracts in parallel, enhancing throughput while maintaining the "100ms and regulated" value proposition.

## Architecture Overview

The multi-TEE pair system consists of these key components:

1. **Coordinator**: A central service that manages TEE pairs, distributes tasks, and ensures consensus.
2. **TEE Controllers**: Individual TEE instances that can be paired together to form a TEE pair.
3. **TEE Pairs**: Two TEE controllers working together to execute the same contract operations and validate each other's results.
4. **Regions**: Logical groupings of TEE pairs, typically organized by geographic region or regulatory domain.

## Setting Up Multi-TEE Execution

### Environment Variables

Configure TEE controllers with these environment variables:

- `USE_COORDINATOR`: Set to "true" to enable coordinator integration
- `COORDINATOR_URL`: URL of the coordinator service (default: "http://localhost:8080")
- `RUN_MODE`: Set to "primary" or "secondary" for integration testing

### Registering TEE Pairs

TEE pairs must be registered with the coordinator before they can execute tasks:

1. Initialize and register each TEE controller with the coordinator
2. Register a TEE pair by specifying a region and secondary worker
3. The TEE pair is now ready to execute tasks

Example code:
```rust
// Initialize and register with coordinator
controller.initialize_coordinator().await?;

// Get available workers
let workers = controller.get_available_workers().await?;

// Register a TEE pair with an available worker
if !workers.is_empty() {
    controller.register_tee_pair("default", &workers[0]).await?;
}
```

## Executing Tasks

When executing tasks in coordinated mode:

1. Tasks are submitted to the coordinator
2. The coordinator assigns the task to an appropriate TEE pair
3. Both TEEs in the pair execute the task independently
4. Results are compared to ensure consensus
5. The agreed-upon result is returned

Example code:
```rust
// Create a payload
let payload = ExecutionPayload {
    params: ExecutionParams {
        id_from: "source".to_string(),
        id_to: "contract_id".to_string(),
        function: "execute".to_string(),
        input: b"store,key,value".to_vec(),
    },
    region_id: Some("default".to_string()),
};

// Execute via coordinator
let result = controller.execute(&payload).await?;
```

## Testing

### Unit Tests

Run the basic multi-TEE tests:
```bash
cargo test -p tee-controller multi_tee
```

### Integration Testing

To run a full integration test with real coordinator:

1. Start the coordinator service
2. Run the secondary controller:
   ```bash
   RUN_MODE=secondary cargo run --bin multi_tee_integration
   ```
3. In another terminal, run the primary controller:
   ```bash
   RUN_MODE=primary cargo run --bin multi_tee_integration
   ```

## State Management

The system ensures state consistency across TEE pairs:

1. Each operation is executed independently by both TEEs in a pair
2. Results are compared to ensure consistency
3. State changes are only committed when both TEEs agree on the result
4. The coordinator maintains a log of all operations for auditability

## Performance Characteristics

The multi-TEE system maintains the "100ms and regulated" value proposition:

- Contract execution time is still kept under 100ms
- Coordination overhead is minimized through efficient communication
- Parallel execution across multiple TEE pairs increases overall throughput
- State conflicts are resolved through a consensus mechanism

## Security Features

The multi-TEE system enhances security through:

1. **Double Execution**: Each operation is executed by two independent TEEs
2. **Mutual Verification**: TEE pairs verify each other's results
3. **Attestation**: All TEEs provide attestation to prove their integrity
4. **Audit Trail**: The coordinator maintains an immutable log of all operations

## Troubleshooting

Common issues:

1. **TEE pair registration fails**: Ensure both TEEs are registered with the coordinator
2. **Task execution times out**: Check network connectivity and coordinator health
3. **State inconsistency**: Examine the logs for potential consensus failures

For more detailed troubleshooting, check the coordinator logs and TEE controller logs.
