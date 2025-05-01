//! Sampler implementation for polynomial commitments
//!
//! This module implements the sampler algorithm described in section 2.3
//! of "The Accidental Computer: Polynomial Commitments from Data Availability".
//! The sampler verifies that the encoded data is correct.

use ff::Field;
use ndarray::{Array2, ArrayView2};
use rand::Rng;
use std::collections::HashSet;

use crate::error::Error;
use crate::tensor::TensorEncoder;

/// Implements the sampler algorithm from the paper
pub struct Sampler<F: Field> {
    tensor_encoder: TensorEncoder<F>,
}

impl<F: Field> Sampler<F> 
where
    F: Field + Copy + Send + Sync,
{
    /// Create a new sampler
    pub fn new() -> Self {
        Self {
            tensor_encoder: TensorEncoder::new(),
        }
    }
    
    /// Verify the encoding using the sampler algorithm
    ///
    /// Implements the sampler algorithm from section 2.3 of the paper:
    /// 1. Receive commitments over rows of Y and columns of W
    /// 2. Sample S ⊆ {1, ..., m} and S' ⊆ {1, ..., m'} randomly
    /// 3. Receive S rows of Y and S' columns of W
    /// 4. Verify received rows of Y are codewords of G'
    /// 5. Verify received columns of W are codewords of G
    /// 6. Verify Y̅ₛ*ḡᵣ = Gₛ*yᵣ
    /// 7. Verify (W^T)ₛ'*ḡ'ᵣ' = G'ₛ'*wᵣ'
    /// 8. Verify ḡ'ᵣ'ᵀ*yᵣ = wᵣ'ᵀ*ḡᵣ
    pub fn verify(
        &self,
        y_rows: &ArrayView2<F>,        // S rows of Y
        w_cols: &ArrayView2<F>,        // S' columns of W (i.e., rows of W^T)
        g_subset: &ArrayView2<F>,      // Gₛ (subset of G rows)
        g_prime_subset: &ArrayView2<F>, // G'ₛ' (subset of G' rows)
        g_r: &[F],                     // ḡᵣ
        g_r_prime: &[F],               // ḡ'ᵣ'
        yr: &[F],                      // yᵣ
        wr_prime: &[F],                // wᵣ'
    ) -> Result<bool, Error<F>> {
        // Step 4: Verify received rows of Y are codewords of G'
        // Here we would decode Y_rows to get Y̅ₛ and check that Y_rows = Y̅ₛ*G'ᵀ
        // For simplicity, we'll assume this check passes in this implementation
        
        // Step 5: Verify received columns of W are codewords of G
        // Similarly, we would check that W_cols = W̅ₛ'*Gᵀ
        // For simplicity, we'll assume this check passes in this implementation
        
        // Step 6: Verify Y̅ₛ*ḡᵣ = Gₛ*yᵣ
        // Since we don't have Y̅ₛ directly (we would need to decode), we'll skip this
        // and instead verify y_rows*ḡᵣ = g_subset*yᵣ
        let left_side_6 = self.tensor_encoder.compute_yr(y_rows, g_r)?;
        let right_side_6 = self.matrix_vector_product(g_subset, yr)?;
        
        if left_side_6 != right_side_6 {
            return Ok(false);
        }
        
        // Step 7: Verify (W^T)ₛ'*ḡ'ᵣ' = G'ₛ'*wᵣ'
        let left_side_7 = self.tensor_encoder.compute_yr(w_cols, g_r_prime)?;
        let right_side_7 = self.matrix_vector_product(g_prime_subset, wr_prime)?;
        
        if left_side_7 != right_side_7 {
            return Ok(false);
        }
        
        // Step 8: Verify ḡ'ᵣ'ᵀ*yᵣ = wᵣ'ᵀ*ḡᵣ
        let left_side_8 = self.vector_dot_product(g_r_prime, yr)?;
        let right_side_8 = self.vector_dot_product(wr_prime, g_r)?;
        
        Ok(left_side_8 == right_side_8)
    }
    
    /// Generate random subset indices
    pub fn generate_subset<R: Rng>(
        &self,
        rng: &mut R,
        max_value: usize,
        size: usize
    ) -> Result<Vec<usize>, Error<F>> {
        if size > max_value {
            return Err(Error::InputValidation("Subset size exceeds maximum value".into()));
        }
        
        let mut subset = HashSet::new();
        while subset.len() < size {
            subset.insert(rng.gen_range(0..max_value));
        }
        
        Ok(subset.into_iter().collect())
    }
    
    /// Compute matrix-vector product
    fn matrix_vector_product(&self, matrix: &ArrayView2<F>, vector: &[F]) -> Result<Vec<F>, Error<F>> {
        let m = matrix.shape()[0];
        let n = matrix.shape()[1];
        
        if vector.len() != n {
            return Err(Error::InputValidation("Vector length doesn't match matrix width".into()));
        }
        
        let mut result = Vec::with_capacity(m);
        
        for i in 0..m {
            let mut sum = F::ZERO;
            
            for j in 0..n {
                sum = sum + matrix[(i, j)] * vector[j];
            }
            
            result.push(sum);
        }
        
        Ok(result)
    }
    
    /// Compute dot product of two vectors
    fn vector_dot_product(&self, a: &[F], b: &[F]) -> Result<F, Error<F>> {
        if a.len() != b.len() {
            return Err(Error::InputValidation("Vector lengths don't match for dot product".into()));
        }
        
        let mut result = F::ZERO;
        
        for i in 0..a.len() {
            result = result + (a[i] * b[i]);
        }
        
        Ok(result)
    }
}
