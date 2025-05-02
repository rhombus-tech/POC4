# Polynomial Commitments TEE Integration

This module integrates the `polynomial_commitments` crate with the TEE controller, providing secure polynomial commitment operations through the trusted execution environment.

## Overview

The polynomial commitments integration adds support for two primary operations:

1. **secure_commit** - Creates a secure polynomial commitment using a data matrix and generator matrices
2. **secure_open_at_point** - Opens a commitment at a specific evaluation point

These operations can be executed through the standard `TeeExecutor` interface by specifying the appropriate function name in the `ExecutionPayload`.

## Integration Methods

There are two ways to integrate polynomial commitments with your existing TEE infrastructure:

### 1. Standalone Controller

Use the `PolynomialController` directly as a standalone `TeeExecutor`:

```rust
// Create and initialize a standalone polynomial controller
let polynomial_controller = PolynomialController::new(TeeType::SGX).await?;

// Use it directly for polynomial commitment operations
let result = polynomial_controller.execute(&payload).await?;
```

### 2. Extend Existing Executor

Add polynomial commitment capabilities to an existing TEE executor using the `extend_tee_executor` helper method:

```rust
// Start with your existing executor (e.g., EnarxController)
let base_executor = EnarxController::new(TeeType::SGX, "/path/to/config", false).await?;

// Create the polynomial controller
let polynomial_controller = PolynomialController::new(TeeType::SGX).await?;

// Combine them into a unified executor
let extended_executor = PolynomialController::extend_tee_executor(
    base_executor, 
    polynomial_controller
).await;

// Use the extended executor which supports both standard contract execution
// and polynomial commitment operations
let result = extended_executor.execute(&payload).await?;
```

## Parameter Handling

The polynomial commitment operations support both parameter formats used in Wasmlanche WebAssembly contracts:

1. **Length-prefixed format** - First 4 bytes are a little-endian u32 length followed by the actual data
2. **Direct format** - Raw data without length prefix

The implementation automatically detects the format being used and processes it accordingly, with robust safety checks to prevent memory vulnerabilities.

## Usage Examples

### Creating a Commitment

```rust
// Prepare your input data (data matrix, g and g_prime_t matrices)
let combined_data = prepare_matrices();

// Create an execution payload
let payload = ExecutionPayload {
    input: combined_data,
    params: ExecutionParams {
        id_to: "polynomial-contract".to_string(),
        function_call: "secure_commit".to_string(),
        detailed_proof: false,
        expected_hash: Vec::new(),
    },
    ..Default::default()
};

// Execute the operation
let result = executor.execute(&payload).await?;
```

### Opening a Commitment

```rust
// Prepare your input data (data matrix, point_r, point_r_prime vectors)
let combined_data = prepare_opening_data();

// Create an execution payload
let payload = ExecutionPayload {
    input: combined_data,
    params: ExecutionParams {
        id_to: "polynomial-contract".to_string(),
        function_call: "secure_open_at_point".to_string(),
        detailed_proof: false,
        expected_hash: Vec::new(),
    },
    ..Default::default()
};

// Execute the operation
let result = executor.execute(&payload).await?;
```

## Security Features

- Robust parameter validation with protection against memory vulnerabilities
- Dual-format parameter handling for compatibility with all client types
- Production-quality cryptographic field arithmetic using `pasta_curves::Fp`
- No unsafe memory access or potential panics
- Comprehensive debug logging for all error cases

## Test Mode

Set the `RUNNING_TESTS=1` environment variable during testing to enable relaxed validation:
- In test mode, any non-empty direct format buffer is accepted
- Length-prefixed format is still validated as normal
- Extremely large length prefixes (e.g., 3.5B byte bug) are still rejected as malicious

## Notes for Integrators

1. When integrating with an existing executor, use the `extend_tee_executor` helper for seamless operation.
2. The polynomial commitment operations don't maintain persistent state; they operate purely on the input data.
3. For production deployment, ensure proper field element serialization matching the `pasta_curves::Fp` representation.
