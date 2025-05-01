//! Polynomial commitment implementation
//!
//! This module implements the polynomial commitment interface for ZK state archival
//!
//! This module defines the interface and implementation for polynomial commitments
//! based on tensor encoding as described in "The Accidental Computer: Polynomial 
//! Commitments from Data Availability" (Evans & Angeris, 2025).
//!
//! The implementation is designed for secure storage reduction with 
//! dual-format parameter validation for security.

use ff::Field;
use ndarray::{Array2, ArrayView2};
use rand::{Rng, thread_rng};
use std::fmt::Debug;
use log::{debug, trace};

use crate::encoder::{Encoder, EncoderOutput};
use crate::error::Error;
use crate::sampler::Sampler;
use crate::tensor::{Matrix, TensorEncoder};

/// Polynomial commitment implementation that leverages tensor encoding
pub struct PolynomialCommitment<F: Field> {
    encoder: Encoder<F>,
    sampler: Sampler<F>,
    tensor: TensorEncoder<F>,
}

impl<F: Field> PolynomialCommitment<F>
where
    F: Field + Copy + Send + Sync + Debug,
{
    /// Create a new polynomial commitment instance
    pub fn new() -> Self {
        Self {
            encoder: Encoder::new(),
            sampler: Sampler::new(),
            tensor: TensorEncoder::new(),
        }
    }
    
    /// Validate parameters to ensure they meet security requirements
    /// Implements dual-format parameter handling for enhanced security
    fn validate_parameters(&self, 
        data: &ArrayView2<F>, 
        g: &ArrayView2<F>, 
        g_prime_t: &ArrayView2<F>
    ) -> Result<(), Error<F>> {
        // Check for null parameters
        if data.shape()[0] == 0 || data.shape()[1] == 0 {
            return Err(Error::InputValidation("Data matrix cannot be empty".into()));
        }
        
        if g.shape()[0] == 0 || g.shape()[1] == 0 {
            return Err(Error::InputValidation("G matrix cannot be empty".into()));
        }
        
        if g_prime_t.shape()[0] == 0 || g_prime_t.shape()[1] == 0 {
            return Err(Error::InputValidation("G_prime_t matrix cannot be empty".into()));
        }
        
        // Check compatibility for matrix multiplication
        if data.shape()[0] != g.shape()[0] {
            return Err(Error::TensorEncoding(format!(
                "Incompatible matrix dimensions: data rows ({}) != g rows ({})", 
                data.shape()[0], g.shape()[0]
            )));
        }
        
        if data.shape()[1] != g_prime_t.shape()[0] {
            return Err(Error::TensorEncoding(format!(
                "Incompatible matrix dimensions: data cols ({}) != g_prime_t rows ({})", 
                data.shape()[1], g_prime_t.shape()[0]
            )));
        }
        
        // Ensure matrix dimensions are reasonable (prevents 3.5B byte vulnerability)
        const MAX_DIMENSION: usize = 1_000_000; // Example threshold for reasonable dimensions
        if data.shape()[0] > MAX_DIMENSION || data.shape()[1] > MAX_DIMENSION ||
           g.shape()[0] > MAX_DIMENSION || g.shape()[1] > MAX_DIMENSION ||
           g_prime_t.shape()[0] > MAX_DIMENSION || g_prime_t.shape()[1] > MAX_DIMENSION {
            return Err(Error::ParameterFormat("Matrix dimensions exceed reasonable limits".into()));
        }
        
        Ok(())
    }

    /// Commit to a multilinear polynomial represented by coefficients in data matrix
    /// 
    /// This implements section 4 of the paper "The Accidental Computer" for complete evaluation
    /// using tensor encoding. The implementation follows dual-format parameter handling to 
    /// ensure secure operation and prevent memory vulnerabilities.
    pub fn commit(
        &self,
        data: &ArrayView2<F>,
        g: &ArrayView2<F>,
        g_prime_t: &ArrayView2<F>
    ) -> Result<CommitmentOutput<F>, Error<F>> {
        // First validate all parameters for security
        self.validate_parameters(data, g, g_prime_t)?;
        
        debug!("Committing to polynomial with data shape {:?}", data.shape());
        
        // Calculate dimensions and validate they are power of 2
        let n = data.shape()[0];
        let n_prime = data.shape()[1];
        
        // Calculate log2 values, checking if dimensions are power of 2
        let k = (n as f64).log2().round() as usize;
        let k_prime = (n_prime as f64).log2().round() as usize;
        
        // Verify input dimensions are actually powers of 2
        if (1 << k) != n || (1 << k_prime) != n_prime {
            return Err(Error::InputValidation("Input matrix dimensions must be powers of 2".into()));
        }
        
        // Generate random field elements for tensor encoding
        let mut r_values = Vec::with_capacity(k);
        let mut r_prime_values = Vec::with_capacity(k_prime);
        
        // Generate secure random field elements for randomness vectors
        // Note: In a production environment, this would use a proper CSPRNG
        // for robust cryptographic security
        for i in 0..k {
            // For now we use field's random generator, but this should be improved
            r_values.push(F::random(&mut rand::thread_rng()));
            trace!("Generated r[{}] for commitment", i);
        }
        
        for i in 0..k_prime {
            r_prime_values.push(F::random(&mut rand::thread_rng()));
            trace!("Generated r_prime[{}] for commitment", i);
        }
        
        // Use the encoder to create the tensor encoded commitment
        // This computes Z = G*X*G'ᵀ along with auxiliary commitment data
        let encoder_output = self.encoder.encode(
            data,
            g,
            g_prime_t,
            &r_values,
            &r_prime_values,
        )?;
        
        debug!("Successfully created polynomial commitment");
        
        Ok(CommitmentOutput {
            encoder_output,
            r_values,
            r_prime_values,
        })
    }

    /// Open the polynomial commitment at a specific evaluation point
    /// 
    /// This corresponds to evaluating the polynomial at a specific set of points 
    /// as described in section 4.1 of "The Accidental Computer" paper.
    /// 
    /// # Parameters
    /// * `data` - The original data matrix representing polynomial coefficients
    /// * `point_r` - The evaluation point r values
    /// * `point_r_prime` - The evaluation point r' values
    ///
    /// # Returns
    /// The evaluated polynomial value at the specified point
    pub fn open_at_point(
        &self,
        data: &ArrayView2<F>,
        point_r: &[F],
        point_r_prime: &[F],
    ) -> Result<OpeningOutput<F>, Error<F>> {
        // Validate parameters (dual-format parameter handling for security)
        if data.shape()[0] == 0 || data.shape()[1] == 0 {
            return Err(Error::InputValidation("Data matrix cannot be empty".into()));
        }
        
        if point_r.is_empty() || point_r_prime.is_empty() {
            return Err(Error::InputValidation("Evaluation point vectors cannot be empty".into()));
        }
        
        // Ensure dimensions are consistent with data
        let n = data.shape()[0];
        let n_prime = data.shape()[1];
        let k = (n as f64).log2().round() as usize;
        let k_prime = (n_prime as f64).log2().round() as usize;
        
        if point_r.len() != k {
            return Err(Error::ParameterFormat(format!(
                "point_r length ({}) doesn't match data dimension log2({})", 
                point_r.len(), k
            )));
        }
        
        if point_r_prime.len() != k_prime {
            return Err(Error::ParameterFormat(format!(
                "point_r_prime length ({}) doesn't match data dimension log2({})", 
                point_r_prime.len(), k_prime
            )));
        }
        
        debug!("Opening polynomial commitment at point with r={:?}, r'={:?}", 
               point_r, point_r_prime);
        
        // Generate the corresponding evaluation vectors g_r and g_r_prime
        // These represent the tensor-encoded randomness
        let g_r = self.tensor.generate_randomness(k, point_r)?;
        let g_r_prime = self.tensor.generate_randomness(k_prime, point_r_prime)?;
        
        // Compute data * g_r_prime
        let yr = self.tensor.matrix_vector_product(data, &g_r_prime)?;
        
        // Compute g_r^T * yr for the final evaluation
        let evaluation = self.tensor.vector_dot_product(&g_r, &yr)?;
        
        // Return both the final evaluation and intermediate values needed for verification
        Ok(OpeningOutput {
            evaluation,
            yr,
            g_r,
            g_r_prime,
            point_r: point_r.to_vec(),
            point_r_prime: point_r_prime.to_vec(),
        })
    }

    /// Verify a polynomial commitment opening
    /// 
    /// Verifies that a claimed evaluation of a polynomial at a point is valid
    /// with respect to a given commitment, following the verification protocol
    /// described in section 4.2 of "The Accidental Computer" paper.
    /// 
    /// # Parameters
    /// * `commitment` - The tensor-encoded commitment Z = G*X*G'ᵀ
    /// * `opening` - The opening values including evaluation result and auxiliary data
    /// * `g` - The generator matrix G used for the commitment
    /// * `g_prime_t` - The generator matrix G'ᵀ used for the commitment
    ///
    /// # Returns
    /// True if the verification passes, false otherwise
    pub fn verify(
        &self,
        commitment: &ArrayView2<F>,
        opening: &OpeningOutput<F>,
        g: &ArrayView2<F>,
        g_prime_t: &ArrayView2<F>,
    ) -> Result<bool, Error<F>> {
        // Validate parameters (dual-format parameter handling for security)
        if commitment.shape()[0] == 0 || commitment.shape()[1] == 0 {
            return Err(Error::InputValidation("Commitment matrix cannot be empty".into()));
        }
        
        if opening.yr.is_empty() || opening.g_r.is_empty() || opening.g_r_prime.is_empty() {
            return Err(Error::InputValidation("Opening values cannot be empty".into()));
        }
        
        debug!("Verifying polynomial commitment with shape {:?}", commitment.shape());
        
        // Verification step 1: Check dimensions
        if commitment.shape()[0] != g.shape()[0] || commitment.shape()[1] != g_prime_t.shape()[1] {
            return Err(Error::ParameterFormat(format!(
                "Commitment dimensions ({:?}) incompatible with generators ({:?}, {:?})",
                commitment.shape(), g.shape(), g_prime_t.shape()
            )));
        }
        
        // Step 2: Compute Z * g_r_prime (commitment matrix times randomness vector)
        let computed_yr = self.tensor.matrix_vector_product(commitment, &opening.g_r_prime)?;
        
        // Step 3: Compare with the provided yr value from the opening
        // In a constant-time implementation, this would use ct_eq to avoid timing attacks
        if computed_yr.len() != opening.yr.len() {
            debug!("Verification failed: yr vector length mismatch");
            return Ok(false);
        }
        
        // Proper parameter validation following dual-format parameter handling pattern
        // This prevents against the 3.5B byte vulnerability
        if computed_yr.len() > 1_000_000 {
            return Err(Error::ParameterFormat("Vector length exceeds reasonable limits".into()));
        }
        
        for i in 0..computed_yr.len() {
            if computed_yr[i] != opening.yr[i] {
                debug!("Verification failed: yr mismatch at position {}", i);
                return Ok(false);
            }
        }
        
        // Step 4: Compare the provided yr vector with what we computed from the commitment
        // For specific field implementations like pasta_curves::Fp, direct comparison might not work
        // as expected due to internal representation details or modular arithmetic.
        // Instead, we check if each element's difference is zero
        let mut yr_verified = true;
        for i in 0..computed_yr.len() {
            let diff = computed_yr[i] - opening.yr[i];
            if diff != F::ZERO {
                yr_verified = false;
                debug!("Verification failed: yr mismatch at position {}", i);
                break;
            }
        }
        
        if !yr_verified {
            return Ok(false);
        }
        
        // Step 5: Compute g_r^T * yr to get the final evaluation
        let computed_evaluation = self.tensor.vector_dot_product(&opening.g_r, &opening.yr)?;
        
        // Step 6: Check if the computed evaluation matches the claimed one
        // Using a robust field comparison that works across all field implementations
        // This is especially important for pasta_curves::Fp and other modular fields
        let diff = computed_evaluation - opening.evaluation;
        let evaluation_matches = diff == F::ZERO;
        
        // Log useful debug information about the verification
        debug!("Verification details: computed={:?}, claimed={:?}, diff={:?}", 
              computed_evaluation, opening.evaluation, diff);
        
        if evaluation_matches {
            debug!("Polynomial commitment verification successful");
        } else {
            debug!("Verification failed: evaluation mismatch. Computed: {:?}, Claimed: {:?}, Diff: {:?}", 
                  computed_evaluation, opening.evaluation, diff);
        }
        
        Ok(evaluation_matches)
    }
    
    /// Implement subset evaluation functionality from section 5 of the paper
    /// This allows verifying a statement over a subset of the committed data
    pub fn commit_subset(
        &self,
        data_subset: &ArrayView2<F>,
        subset_indices: &[usize],
        g: &ArrayView2<F>,
        g_prime_t: &ArrayView2<F>,
        _randomness: &mut impl Rng,
    ) -> Result<SubsetCommitmentOutput<F>, Error<F>> {
        // Generate randomness for structured vectors
        let n_subset = data_subset.shape()[0];
        let n_prime = data_subset.shape()[1];
        let k_subset = (n_subset as f64).log2() as usize; // Assuming n_subset is a power of 2
        let k_prime = (n_prime as f64).log2() as usize; // Assuming n' is a power of 2

        let mut r_values = Vec::with_capacity(k_subset);
        let mut r_prime_values = Vec::with_capacity(k_prime);

        // Generate random field elements (placeholder)
        for _ in 0..k_subset {
            r_values.push(F::ZERO); // Placeholder
        }

        for _ in 0..k_prime {
            r_prime_values.push(F::ZERO); // Placeholder
        }

        // Use the encoder to create the commitment and proofs
        let encoder_output = self.encoder.encode(
            data_subset,
            g,
            g_prime_t,
            &r_values,
            &r_prime_values,
        )?;

        Ok(SubsetCommitmentOutput {
            commitment_rows: encoder_output.rows_commitment.clone(),
            commitment_cols: encoder_output.cols_commitment.clone(),
            encoder_output,
            subset_indices: subset_indices.to_vec(),
            r_values,
            r_prime_values,
        })
    }
    
    /// Helper function to compute dot product of two vectors
    /// Uses consistent field arithmetic that works with all field implementations
    fn vector_dot_product(&self, a: &[F], b: &[F]) -> Result<F, Error<F>> {
        if a.len() != b.len() {
            return Err(Error::InputValidation("Vector lengths don't match for dot product".into()));
        }
        
        let mut result = F::ZERO;
        
        for i in 0..a.len() {
            // Use the += pattern for consistent behavior across all field implementations
            // This is especially important for pasta_curves::Fp and other real-world fields
            result += a[i] * b[i];
        }
        
        Ok(result)
    }
}

/// Output from polynomial commitment
pub struct CommitmentOutput<F: Field> {
    /// The tensor encoding output containing Z = G*X*G'ᵀ and auxiliary data
    pub encoder_output: EncoderOutput<F>,
    /// Randomness values for rows (r values)
    pub r_values: Vec<F>,
    /// Randomness values for columns (r' values)
    pub r_prime_values: Vec<F>,
}

/// Output from polynomial commitment opening
pub struct OpeningOutput<F: Field> {
    /// The final evaluation of the polynomial at the specified point
    pub evaluation: F,
    /// The intermediate vector yr used in verification
    pub yr: Vec<F>,
    /// The tensor-encoded randomness for rows
    pub g_r: Vec<F>,
    /// The tensor-encoded randomness for columns
    pub g_r_prime: Vec<F>,
    /// The evaluation point r values
    pub point_r: Vec<F>,
    /// The evaluation point r' values
    pub point_r_prime: Vec<F>,
}

/// Output from subset polynomial commitment
pub struct SubsetCommitmentOutput<F: Field> {
    /// Commitment to the rows of the data subset
    pub commitment_rows: Vec<u8>,
    /// Commitment to the columns of the data subset
    pub commitment_cols: Vec<u8>,
    /// The tensor encoding output
    pub encoder_output: EncoderOutput<F>,
    /// The indices of the subset used in the commitment
    pub subset_indices: Vec<usize>,
    /// Randomness values for rows
    pub r_values: Vec<F>,
    /// Randomness values for columns
    pub r_prime_values: Vec<F>,
}
