# Polynomial Commitment TEE Integration

This document describes how to use the polynomial commitment operations integrated with the TEE controller.

## Overview

The polynomial commitment module provides two secure operations:
1. `secure_commit` - Creates a commitment to a polynomial matrix
2. `secure_open_at_point` - Opens a commitment at a specific evaluation point

These operations now run inside the TEE (Trusted Execution Environment) with full hardware security and attestation support.

## Usage

### Calling Polynomial Operations

You can invoke polynomial operations through the standard TEE execution interface. The function name determines which operation is performed:

```rust
// Example: Create an execution payload for secure_commit
let payload = ExecutionPayload {
    input: your_encoded_data,  // Matrix data in the proper format
    params: ExecutionParams {
        id_to: "your-contract-id".to_string(),
        function_call: "secure_commit".to_string(), // Use "secure_commit" or "secure_open_at_point"
        detailed_proof: false,
        expected_hash: Vec::new(),
    },
    operation_id: None,
    previous_operation_id: None,
    operation_context: None,
    region_id: Some("polynomial-region".to_string()),
    target_tee: None,
    tee_type: Some("SGX".to_string()),
    allow_fallback: None,
};

// Execute using the TeeExecutor
let result = executor.execute(&payload).await?;
```

### Data Format

Both operations support dual parameter formats for maximum compatibility:

#### 1. Length-Prefixed Format

Data is prefixed with a 4-byte (u32) length header:
- First 4 bytes: length of data in little-endian format
- Remaining bytes: Actual data

```
[u32 length][data bytes]
```

#### 2. Direct Format

Data is provided directly without a length prefix. This is used in some testing scenarios and when fixed-size data is expected.

### Matrix/Vector Encoding

Matrices and vectors use a standardized encoding format:

#### Matrix Format:
```
[u32 rows][u32 cols][Fp elements in row-major order]
```

#### Vector Format:
```
[u32 length][Fp elements]
```

Each Fp element is 32 bytes, encoded using the pasta_curves representation.

## Security Considerations

- All inputs are validated with proper bounds checking
- Protection against the 3.5B byte attack is implemented
- No panics or unsafe memory access
- Comprehensive debug logging

## Example: Secure Commit Operation

```rust
// Generate a random matrix
let data_matrix = // ... your matrix data
let g_matrix = // ... your G matrix
let g_prime_t_matrix = // ... your G' transposed matrix

// Encode matrices
let data_bytes = encode_matrix_to_bytes(&data_matrix.view());
let g_bytes = encode_matrix_to_bytes(&g_matrix.view());
let g_prime_t_bytes = encode_matrix_to_bytes(&g_prime_t_matrix.view());

// Combine data for input
let mut combined_data = Vec::new();
combined_data.extend_from_slice(&data_bytes);
combined_data.extend_from_slice(&g_bytes);
combined_data.extend_from_slice(&g_prime_t_bytes);

// Create payload and execute
let payload = ExecutionPayload {
    input: combined_data,
    params: ExecutionParams {
        function_call: "secure_commit".to_string(),
        // ... other params
    },
    // ... other fields
};

let result = executor.execute(&payload).await?;
// Result contains the commitment
```

## Example: Secure Open At Point Operation

```rust
// Prepare your data matrix and evaluation points
let data_matrix = // ... your matrix data
let point_r = // ... your evaluation point r
let point_r_prime = // ... your evaluation point r'

// Encode data and points
let data_bytes = encode_matrix_to_bytes(&data_matrix.view());
let point_r_bytes = encode_vector_to_bytes(&point_r);
let point_r_prime_bytes = encode_vector_to_bytes(&point_r_prime);

// Combine data
let mut combined_data = Vec::new();
combined_data.extend_from_slice(&data_bytes);
combined_data.extend_from_slice(&point_r_bytes);
combined_data.extend_from_slice(&point_r_prime_bytes);

// Create payload and execute
let payload = ExecutionPayload {
    input: combined_data,
    params: ExecutionParams {
        function_call: "secure_open_at_point".to_string(),
        // ... other params
    },
    // ... other fields
};

let result = executor.execute(&payload).await?;
// Result contains the opening proof
```

## Error Handling

The polynomial operations return standard TEE errors with descriptive messages when problems occur:

- `TeeError::InvalidFunction` - If an unknown function is called
- `TeeError::ExecutionFailed` - If the operation fails during processing
- `TeeError::InvalidParameters` - If parameters are invalid or malformed
