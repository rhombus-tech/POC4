# Polynomial Commitment TEE Integration

This document describes how to use the polynomial commitment operations integrated with the TEE controller.

## The Accidental Computer

Our polynomial commitment implementation is based on "The Accidental Computer: Polynomial Commitments from Data Availability" (Evans & Angeris, 2025). The key insight of this approach is that tensor-based encoding creates an "accidental computer" that efficiently computes multivariate polynomial evaluations.

### Core Concepts

The "accidental computer" leverages tensor operations (Z = G*X*G'ᵀ) to implement:

1. **Efficient Commitment** - Create compact commitments to large polynomial data
2. **Verifiable Evaluation** - Allow anyone to verify evaluations at specific points without seeing the entire polynomial
3. **Succinct Proofs** - Verification requires minimal data exchange

This approach provides exceptional space efficiency and security guarantees while maintaining hardware-level performance through our TEE implementation.

### TEE-Enhanced Security Model

Our implementation enhances the base "accidental computer" with hardware-level security through our TEE mesh network:

1. **Multi-TEE Architecture** - Operations are executed across a mesh of different TEE types (SGX, SEV, TDX)
2. **Cross-Attestation** - Commitments include attestation proofs from multiple TEE types
3. **Hardware Diversity** - Using different hardware TEEs requires an attacker to compromise multiple architectures
4. **Verifiable Execution** - Every polynomial operation comes with hardware attestation guarantees

### Why TEEs Enhance the Accidental Computer

TEEs and the accidental computer polynomial commitment system create a powerful synergy by addressing different aspects of security and trust:

1. **Complementary Trust Models**
   - The accidental computer provides *mathematical guarantees* (cryptographic security)
   - TEEs provide *hardware guarantees* (physical isolation)
   - Together they create two independent verification layers an attacker must overcome

2. **Protection of Private Inputs**
   - One vulnerability of polynomial commitments is that the original data (the polynomial coefficients) must be kept private
   - TEEs provide hardware-enforced memory isolation to protect this data during computation
   - Even a compromised operating system cannot access the raw polynomial data

3. **Verifiable Parameter Generation**
   - Many polynomial commitment schemes are vulnerable to parameter subversion attacks
   - TEEs can generate and attest that parameters were correctly created
   - The attestation proofs verify that no backdoors exist in the commitment parameters

4. **Deterministic Execution**
   - TEEs ensure that the polynomial operations execute exactly as intended
   - This prevents side-channel attacks that could extract information about the polynomial
   - Critical for operations where timing or power analysis could leak information

5. **Cross-Validation Architecture**
   - Our multi-TEE approach runs the same polynomial operation on different hardware
   - This ensures that a vulnerability in one TEE implementation doesn't compromise security
   - Creates a "defense in depth" approach to securing the polynomial commitments

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
