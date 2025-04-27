use wasmlanche::{
    public, Address, Context, 
    state_schema,
    borsh::{BorshSerialize, BorshDeserialize}
};
use tee_interface::prelude::*;
use num_bigint::{BigUint, RandBigInt};
use num_traits::{One, Zero};
use num_integer::Integer;
use sha2::{Sha256, Digest};
use rand::thread_rng;
use std::convert::TryInto;
use std::mem::size_of;

// RSA Security Parameters
const RSA_KEY_SIZE_BITS: usize = 2048;
const RSA_EXPONENT: u64 = 65537; // Common RSA public exponent (e)
const MAX_WITNESS_AGE: u64 = 7 * 24 * 60 * 60; // 1 week
const DEFAULT_BATCH_SIZE: u64 = 100;
const DEFAULT_BATCH_TIMEOUT: u64 = 10; // seconds

// Parameter format configuration
const ENABLE_LENGTH_PREFIX_FORMAT: bool = true; // Support 4-byte length-prefixed format
const ENABLE_DIRECT_FORMAT: bool = true;        // Support direct format without length prefix
const MAX_PARAM_SIZE: usize = 1024 * 1024;      // 1MB max parameter size

// Serialization helpers for BigUint
pub mod bigint_serialization {
    use num_bigint::BigUint;
    use wasmlanche::borsh::{BorshSerialize, BorshDeserialize};
    use std::io::{Read, Write};

    impl BorshSerialize for BigUint {
        fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
            let bytes = self.to_bytes_le();
            let len = bytes.len() as u32;
            len.serialize(writer)?;
            writer.write_all(&bytes)?;
            Ok(())
        }
    }

    impl BorshDeserialize for BigUint {
        fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
            let len = u32::deserialize_reader(reader)?;
            let mut bytes = vec![0u8; len as usize];
            reader.read_exact(&mut bytes)?;
            Ok(BigUint::from_bytes_le(&bytes))
        }
    }
}

state_schema! {
    /// RSA Accumulator public parameters
    RsaParams => RsaAccumulatorParams,
    /// Current RSA accumulator value
    RsaAccumulatorValue => BigUint,
    /// Per-executor witness data
    RsaWitness(Address) => RsaAccumulatorWitness,
    /// Per-executor attestation record
    AttestationRecord(Address) => AttestationRecord,
    /// Batch accumulation buffer
    BatchBuffer => Vec<AccumulatorElement>,
    /// Total size of accumulator
    AccumulatorSize => u64,
    /// Contract parameters
    Parameters => AccumulatorParams,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct RsaAccumulatorParams {
    pub modulus: BigUint,       // RSA modulus (n)
    pub exponent: BigUint,      // RSA public exponent (e)
    pub initialized: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AccumulatorParams {
    pub max_size: u64,
    pub max_witness_age: u64,
    pub min_attestations: u64,
    pub batch_size: u64,
    pub batch_timeout_secs: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AccumulatorElement {
    pub executor: Address,
    pub measurement: [u8; 32],
    pub enclave_type: EnclaveType,
    pub timestamp: u64,
    pub format: Option<String>, // Track which format was used for processing
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct RsaAccumulatorWitness {
    pub value: BigUint,               // RSA witness value (constant size)
    pub element: AccumulatorElement,  // The element this witness proves
    pub last_update: u64,             // Timestamp of last update
    pub batch_id: Option<u64>,        // ID of batch (if processed in batch)
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AttestationRecord {
    pub executor: Address,
    pub last_attestation: u64,
    pub attestation_count: u64,
    pub sgx_measurement: Option<[u8; 32]>,
    pub sev_measurement: Option<[u8; 32]>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct BatchInfo {
    pub batch_id: u64,
    pub elements: Vec<AccumulatorElement>,
    pub timestamp: u64,
}

/// Initialize the RSA accumulator with secure parameters
#[public]
pub fn init(context: &mut Context, params: Option<AccumulatorParams>) -> Result<(), TeeError> {
    // Check if already initialized
    if let Some(rsa_params) = context.get(RsaParams)? {
        if rsa_params.initialized {
            return Err(TeeError::InitializationError("Already initialized".into()));
        }
    }

    // Generate or use provided RSA parameters
    let rsa_params = generate_rsa_params()?;
    
    // Use provided params or defaults for accumulator settings
    let params = params.unwrap_or(AccumulatorParams {
        max_size: 1000000,
        max_witness_age: MAX_WITNESS_AGE,
        min_attestations: 2,
        batch_size: DEFAULT_BATCH_SIZE,
        batch_timeout_secs: DEFAULT_BATCH_TIMEOUT,
    });

    // Initial accumulator value is 2
    let initial_value = BigUint::from(2u64);
    
    // Store initial state
    context.store((
        (RsaParams, rsa_params),
        (Parameters, params),
        (AccumulatorSize, 0u64),
        (RsaAccumulatorValue, initial_value),
        (BatchBuffer, Vec::new()),
    ))?;

    Ok(())
}

/// Generate secure RSA parameters
fn generate_rsa_params() -> Result<RsaAccumulatorParams, TeeError> {
    // In a real implementation, we would generate proper RSA parameters
    // For this example, we'll use dummy values that are secure enough for testing
    
    // WARNING: In production, use a proper RSA key generation library
    let modulus = BigUint::from(1234567890123456789u64);
    let exponent = BigUint::from(RSA_EXPONENT);
    
    Ok(RsaAccumulatorParams {
        modulus,
        exponent,
        initialized: true,
    })
}

/// Register an attestation with the accumulator
#[public]
pub fn register_attestation(
    context: &mut Context,
    attestation: AttestationReport,
) -> Result<(), TeeError> {
    // Get the current batch information
    let mut batch = context.get_or_init_mut::<BatchInfo>()?;
    
    // Parse measurement data using dual-format validation
    let (validated_measurement, format) = parse_dual_format_parameters(&attestation.measurement)?;
    
    // Log which format was detected
    trace_log(format!("Processing attestation from {} using {} format", 
                      hex_str(&attestation.executor), format).as_str());
    
    // Create accumulator element from attestation with validated measurement
    let element = AccumulatorElement {
        executor: Address::from_bytes(&attestation.executor),
        measurement: validated_measurement,
        enclave_type: attestation.enclave_type,
        timestamp: attestation.timestamp,
        format: Some(format),
    };

    // Add to batch buffer
    let mut batch_buffer = context.get(BatchBuffer)?.unwrap_or_default();
    batch_buffer.push(element.clone());
    
    // Update record
    let executor = context.actor();
    let mut record = context.get(AttestationRecord(executor))?.unwrap_or(AttestationRecord {
        executor,
        last_attestation: 0,
        attestation_count: 0,
        sgx_measurement: None,
        sev_measurement: None,
    });

    // Update measurements based on enclave type
    match attestation.enclave_type {
        EnclaveType::IntelSGX => {
            record.sgx_measurement = Some(attestation.measurement);
        },
        EnclaveType::AMDSEV => {
            record.sev_measurement = Some(attestation.measurement);
        }
    }

    // Update record
    record.last_attestation = attestation.timestamp;
    record.attestation_count += 1;
    
    // Check if we need to process the batch
    let process_batch = batch_buffer.len() >= params.batch_size as usize || 
                        (batch_buffer.len() > 0 && 
                         batch_buffer[0].timestamp + params.batch_timeout_secs < context.timestamp());
    
    // Create an RSA witness for this element immediately
    // This allows the executor to continue without waiting for batch completion
    let acc_value = context.get(RsaAccumulatorValue)?.unwrap_or_else(|| BigUint::from(2u64));
    let prime = hash_to_prime(&element);
    
    // Create RSA witness: A^(1/x) mod N where x is the prime for this element
    let witness = RsaAccumulatorWitness {
        value: compute_witness(&acc_value, &prime, &rsa_params.modulus)?,
        element,
        last_update: attestation.timestamp,
        batch_id: None, // Will be updated during batch processing if needed
    };
    
    // Store state
    context.store((
        (AttestationRecord(executor), record),
        (RsaWitness(executor), witness),
        (BatchBuffer, batch_buffer.clone()),
    ))?;
    
    // Process batch if needed
    if process_batch {
        process_batch_update(context)?;
    }

    Ok(())
}

/// Force processing of the current batch
#[public]
pub fn force_batch_update(context: &mut Context) -> Result<(), TeeError> {
    process_batch_update(context)
}

/// Process a batch of attestations
fn process_batch_update(context: &mut Context) -> Result<(), TeeError> {
    let mut batch_buffer = context.get(BatchBuffer)?.unwrap_or_default();
    if batch_buffer.is_empty() {
        return Ok(());
    }
    
    let params = context.get(Parameters)?.ok_or(TeeError::InitializationError("Not initialized".into()))?;
    let rsa_params = context.get(RsaParams)?.ok_or(TeeError::InitializationError("RSA not initialized".into()))?;
    let mut acc_value = context.get(RsaAccumulatorValue)?.unwrap_or_else(|| BigUint::from(2u64));
    let mut acc_size = context.get(AccumulatorSize)?.unwrap_or(0);
    
    if acc_size + batch_buffer.len() as u64 > params.max_size {
        return Err(TeeError::AttestationError("Accumulator would exceed maximum size".into()));
    }
    
    // Create a batch update
    let batch_id = context.timestamp(); // Use timestamp as batch ID
    
    // Hash all elements to primes and multiply them together
    let mut product = BigUint::one();
    let primes: Vec<BigUint> = batch_buffer.iter()
        .map(|element| hash_to_prime(element))
        .collect();
    
    for prime in &primes {
        product = &product * prime;
    }
    
    // Update accumulator: acc_value = acc_value^product mod N
    acc_value = acc_value.modpow(&product, &rsa_params.modulus);
    acc_size += batch_buffer.len() as u64;
    
    // Update all witnesses for elements in the batch
    for (i, element) in batch_buffer.iter().enumerate() {
        let executor = element.executor;
        
        // For each element, compute the witness
        // Unlike the individual case, batch witnesses are more complex
        let prime = &primes[i];
        let exponent = &product / prime; // product / prime
        let witness_value = compute_witness(&acc_value, &exponent, &rsa_params.modulus)?;
        
        let witness = RsaAccumulatorWitness {
            value: witness_value,
            element: element.clone(),
            last_update: context.timestamp(),
            batch_id: Some(batch_id),
        };
        
        context.store((RsaWitness(executor), witness))?;
    }
    
    // Clear batch buffer and update accumulator state
    context.store((
        (RsaAccumulatorValue, acc_value),
        (AccumulatorSize, acc_size),
        (BatchBuffer, Vec::new()),
    ))?;
    
    Ok(())
}

/// Verify attestation across platforms
#[public]
pub fn verify_attestation(
    context: &mut Context,
    sgx_attestation: AttestationReport,
    sev_attestation: AttestationReport,
) -> Result<bool, TeeError> {
    let executor = context.actor();
    
    // Get executor record
    let record = context.get(AttestationRecord(executor))?.ok_or(
        TeeError::AttestationError("Executor not registered".into())
    )?;

    // Verify attestation freshness
    let current_time = context.timestamp();
    if current_time - record.last_attestation > MAX_WITNESS_AGE {
        return Err(TeeError::AttestationError("Attestation too old".into()));
    }

    // Verify witness exists and is valid
    let witness = context.get(RsaWitness(executor))?.ok_or(
        TeeError::AttestationError("No witness found".into())
    )?;

    // Verify measurements match records
    if let Some(sgx_meas) = record.sgx_measurement {
        if sgx_attestation.measurement != sgx_meas {
            return Err(TeeError::AttestationError("SGX measurement mismatch".into()));
        }
    }

    if let Some(sev_meas) = record.sev_measurement {
        if sev_attestation.measurement != sev_meas {
            return Err(TeeError::AttestationError("SEV measurement mismatch".into()));
        }
    }

    // Verify witness value is valid using RSA verification
    let rsa_params = context.get(RsaParams)?.ok_or(TeeError::InitializationError("RSA not initialized".into()))?;
    let acc_value = context.get(RsaAccumulatorValue)?.unwrap_or_else(|| BigUint::from(2u64));
    
    if !verify_rsa_witness(&witness, &acc_value, &rsa_params) {
        return Err(TeeError::AttestationError("Invalid RSA witness".into()));
    }

    Ok(true)
}

/// Verify the given witness against the accumulator value
fn verify_rsa_witness(
    witness: &RsaAccumulatorWitness,
    acc_value: &BigUint,
    rsa_params: &RsaAccumulatorParams
) -> bool {
    // Hash the element to a prime
    let prime = hash_to_prime(&witness.element);
    
    // Verify: witness^prime mod N == acc_value
    let result = witness.value.modpow(&prime, &rsa_params.modulus);
    result == *acc_value
}

/// Hash an element to a prime number in the range [0, N)
pub fn hash_to_prime(element: &AccumulatorElement) -> BigUint {
    // Hash the element's measurement with format awareness
    let mut hasher = Sha256::new();
    hasher.update(element.executor.as_bytes());
    hasher.update(&element.measurement);
    hasher.update(element.enclave_type.to_string().as_bytes());
    hasher.update(element.timestamp.to_le_bytes());
    
    // Include format information if available
    if let Some(format) = &element.format {
        hasher.update(format.as_bytes());
    }
    
    let hash_result = hasher.finalize();
    let mut base_num = BigUint::from_bytes_be(&hash_result);
    
    // Find the next prime number
    while !is_prime(&base_num) {
        base_num += 1u32;
    }
    
    base_num
}

/// Compute an RSA witness for an element
fn compute_witness(
    acc_value: &BigUint,
    exponent: &BigUint,
    modulus: &BigUint
) -> Result<BigUint, TeeError> {
    // In RSA, the witness is acc_value^(1/prime) mod N
    // For batch operations, exponent is product/prime
    // This is a modular exponentiation: acc_value^exponent mod modulus
    Ok(acc_value.modpow(exponent, modulus))
}

/// Compute batch witnesses for multiple elements
fn compute_batch_witnesses(
    acc_value: &BigUint,
    elements: &[AccumulatorElement],
    modulus: &BigUint
) -> Result<Vec<(Address, RsaAccumulatorWitness)>, TeeError> {
    // Hash all elements to primes
    let primes: Vec<BigUint> = elements.iter()
        .map(|element| hash_to_prime(element))
        .collect();
    
    // Compute product of all primes
    let product = primes.iter().fold(BigUint::one(), |acc, p| acc * p);
    
    // For each element, compute the witness
    let mut results = Vec::with_capacity(elements.len());
    let batch_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    for (i, element) in elements.iter().enumerate() {
        let executor = element.executor;
        
        // For each element, compute the witness
        // Unlike the individual case, batch witnesses are more complex
        let prime = &primes[i];
        let exponent = &product / prime; // product / prime (this is faster than individual witnesses)
        let witness_value = compute_witness(&acc_value, &exponent, modulus)?;
        
        results.push((
            element.executor,
            RsaAccumulatorWitness {
                value: witness_value,
                element: element.clone(),
                last_update: batch_id,
                batch_id: Some(batch_id),
            }
        ));
    }
    
    Ok(results)
}

/// Parse parameters using dual-format validation (length-prefixed or direct)
/// Returns validated data and the format used
pub fn parse_dual_format_parameters(data: &[u8]) -> Result<([u8; 32], String), TeeError> {
    // Quick validation for empty data
    if data.is_empty() {
        return Err(TeeError::InvalidInput("Empty parameter data".into()));
    }
    
    // If data length < 4 bytes, can't be length-prefixed
    if data.len() < 4 {
        if ENABLE_DIRECT_FORMAT {
            // Use direct format - copy data (with proper bounds checking)
            return Ok((copy_to_fixed_array(data)?, "direct".into()));
        }
        return Err(TeeError::InvalidInput(
            "Data too short for length prefix and direct format not supported".into()
        ));
    }
    
    // Try to interpret first 4 bytes as length prefix
    if ENABLE_LENGTH_PREFIX_FORMAT {
        let length_bytes: [u8; 4] = data[0..4].try_into().map_err(|_| {
            TeeError::InvalidInput("Failed to read length prefix".into())
        })?;
        let length = u32::from_le_bytes(length_bytes) as usize;
        
        // Validate length is reasonable and matches the data
        if length > 0 && length <= MAX_PARAM_SIZE && length == data.len() - 4 {
            // Valid length-prefixed format
            let param_data = &data[4..]; // Skip length prefix
            return Ok((copy_to_fixed_array(param_data)?, "length-prefixed".into()));
        }
    }
    
    // No valid length prefix or length-prefix not supported, use direct format
    if ENABLE_DIRECT_FORMAT {
        return Ok((copy_to_fixed_array(data)?, "direct".into()));
    }
    
    Err(TeeError::InvalidInput(
        "Invalid parameter format: not a valid length prefix and direct format not supported".into()
    ))
}

/// Safely copy data into a fixed-size array with bounds checking
fn copy_to_fixed_array(data: &[u8]) -> Result<[u8; 32], TeeError> {
    let mut result = [0u8; 32];
    let copy_len = std::cmp::min(data.len(), 32);
    
    if copy_len < 32 && data.len() < 32 {
        trace_log(format!("Warning: Input data length {} is less than required 32 bytes", data.len()).as_str());
    } else if data.len() > 32 {
        trace_log(format!("Warning: Input data length {} exceeds 32 bytes, truncating", data.len()).as_str());
    }
    
    // Safe copy with bounds checking
    result[0..copy_len].copy_from_slice(&data[0..copy_len]);
    Ok(result)
}

/// Helper function to print binary data as hex string
fn hex_str(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<String>>()
        .join("")
}

/// Helper function to send trace logs
fn trace_log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasmlanche::imports::trace;
        trace(message);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        println!("{}", message);
    }
}

mod tests {
    use super::*;
    use wasmlanche::simulator::{Simulator, SimpleState};
    
    /// Set up a test environment
    fn setup() -> (Simulator, Address) {
        let mut state = SimpleState::new();
        let mut sim = Simulator::new(&mut state);
        
        let params = AccumulatorParams {
            max_size: 1000,
            max_witness_age: MAX_WITNESS_AGE,
            min_attestations: 2,
            batch_size: 10,
            batch_timeout_secs: 5,
        };

        let ctx = &mut sim;
        init(ctx, Some(params)).unwrap();

        let executor = Address::new([1; 33]);
        sim.set_actor(executor);

        (sim, executor)
    }

    #[test]
    fn test_batch_processing() {
        let (mut sim, executor) = setup();
        let ctx = &mut sim;
        
        // Create multiple attestations
        for i in 0..15 {
            let attestation = AttestationReport {
                enclave_type: if i % 2 == 0 { EnclaveType::IntelSGX } else { EnclaveType::AMDSEV },
                measurement: [i as u8; 32],
                timestamp: ctx.timestamp() + i,
                platform_data: vec![i as u8],
            };
            
            register_attestation(ctx, attestation).unwrap();
        }
        
        // Verify that batching occurred
        let batch_buffer = ctx.get(BatchBuffer).unwrap().unwrap();
        assert!(batch_buffer.len() < 15, "Should have processed at least one batch");
        
        // Force batch processing of remaining items
        force_batch_update(ctx).unwrap();
        
        // Verify that all items were processed
        let batch_buffer = ctx.get(BatchBuffer).unwrap().unwrap();
        assert_eq!(batch_buffer.len(), 0, "All items should be processed");
        
        // Check accumulator size
        let acc_size = ctx.get(AccumulatorSize).unwrap().unwrap();
        assert_eq!(acc_size, 15, "Accumulator should have 15 items");
    }
    
    #[test]
    fn test_dual_attestation_verification() {
        let (mut sim, contract_addr) = setup();
        
        // Test with different parameter formats
        
        // 1. Create SGX attestation with length-prefixed format
        // Format: [4-byte length][32-byte measurement]
        let mut sgx_measurement = Vec::new();
        let measurement_data = [2u8; 32];
        sgx_measurement.extend_from_slice(&(measurement_data.len() as u32).to_le_bytes()); // Length prefix
        sgx_measurement.extend_from_slice(&measurement_data);
        
        let sgx_attestation = AttestationReport {
            executor: [1u8; 32],
            measurement: copy_to_fixed_array(&sgx_measurement).unwrap(),
            enclave_type: EnclaveType::SGX,
            timestamp: 1000,
        };
        
        // 2. Create SEV attestation with direct format (no length prefix)
        let sev_attestation = AttestationReport {
            executor: [3u8; 32],
            measurement: [4u8; 32], // Direct format without length prefix
            enclave_type: EnclaveType::SEV,
            timestamp: 1000,
        };

        // Register both attestations
        register_attestation(ctx, sgx_attestation).unwrap();
        register_attestation(ctx, sev_attestation).unwrap();
        register_attestation(ctx, sev_attestation.clone()).unwrap();
        
        // Force batch processing
        force_batch_update(ctx).unwrap();

        // Verify attestations
        assert!(verify_attestation(ctx, sgx_attestation, sev_attestation).unwrap());
    }
    
    #[test]
    fn test_performance() {
        let (mut sim, executor) = setup();
        let ctx = &mut sim;
        
        // Batch size for performance test
        const BATCH_SIZE: usize = 100;
        
        // Prepare batch of attestations
        let mut attestations = Vec::with_capacity(BATCH_SIZE);
        for i in 0..BATCH_SIZE {
            attestations.push(AttestationReport {
                enclave_type: if i % 2 == 0 { EnclaveType::IntelSGX } else { EnclaveType::AMDSEV },
                measurement: [i as u8; 32],
                timestamp: ctx.timestamp() + i as u64,
                platform_data: vec![i as u8],
            });
        }
        
        // Measure time for batch processing
        let start = std::time::Instant::now();
        
        for attestation in attestations {
            register_attestation(ctx, attestation).unwrap();
        }
        
        // Force process any remaining items
        force_batch_update(ctx).unwrap();
        
        let duration = start.elapsed();
        println!("Processing {} attestations took: {:?}", BATCH_SIZE, duration);
        
        // Check accumulator size
        let acc_size = ctx.get(AccumulatorSize).unwrap().unwrap();
        assert_eq!(acc_size, BATCH_SIZE as u64, "Accumulator should have correct size");
    }
}
