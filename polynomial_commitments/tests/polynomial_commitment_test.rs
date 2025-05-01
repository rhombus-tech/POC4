//! Integration tests for polynomial commitments
//! 
//! These tests validate the complete polynomial commitment process,
//! from encoding through verification.

use ndarray::{array, Array2, ArrayView2};
use polynomial_commitments::polynomial::PolynomialCommitment;
use polynomial_commitments::error::Error;
use rand::SeedableRng;
use rand::rngs::StdRng;

// Re-use the test field implementation from tensor_test.rs
mod test_field {
    use ff::{Field, PrimeField};
    use std::ops::{Add, Mul, Sub, Neg};
    
    // Simple field implementation for testing - GF(7)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TestField(pub u8);
    
    impl TestField {
        pub fn new(value: u8) -> Self {
            Self(value % 7)
        }
    }
    
    impl Add for TestField {
        type Output = Self;
        
        fn add(self, rhs: Self) -> Self::Output {
            Self::new(self.0.wrapping_add(rhs.0))
        }
    }
    
    impl Sub for TestField {
        type Output = Self;
        
        fn sub(self, rhs: Self) -> Self::Output {
            let result = if self.0 >= rhs.0 {
                self.0 - rhs.0
            } else {
                7 + self.0 - rhs.0
            };
            Self(result)
        }
    }
    
    impl Mul for TestField {
        type Output = Self;
        
        fn mul(self, rhs: Self) -> Self::Output {
            Self::new((self.0 as u16 * rhs.0 as u16) as u8)
        }
    }
    
    impl Neg for TestField {
        type Output = Self;
        
        fn neg(self) -> Self::Output {
            if self.0 == 0 {
                self
            } else {
                Self(7 - self.0)
            }
        }
    }
    
    impl Field for TestField {
        const ZERO: Self = Self(0);
        const ONE: Self = Self(1);
        
        fn random(_rng: impl rand::RngCore) -> Self {
            // This is not random, just for testing
            Self(3)
        }
        
        fn square(&self) -> Self {
            self.mul(*self)
        }
        
        fn double(&self) -> Self {
            self.add(*self)
        }
        
        fn invert(&self) -> Option<Self> {
            if self.0 == 0 {
                None
            } else {
                // Brute force inversion - fine for testing with small fields
                for i in 1..7 {
                    if (self.0 as u16 * i as u16) % 7 == 1 {
                        return Some(Self(i));
                    }
                }
                None
            }
        }
        
        fn sqrt(&self) -> Option<Self> {
            // This is a simplified implementation - only works for specific values
            match self.0 {
                0 => Some(Self(0)),
                1 => Some(Self(1)),
                4 => Some(Self(2)),
                2 => Some(Self(3)), // This is actually not correct for GF(7), but OK for tests
                _ => None,
            }
        }
        
        fn sqrt_ratio(_num: &Self, _div: &Self) -> (Choice, Self) {
            // Simplified implementation for testing
            (Choice::from(1u8), Self(1))
        }
    }
    
    // Simple Choice type for testing
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Choice(pub u8);
    
    impl Choice {
        pub fn from(value: u8) -> Self {
            Self(value)
        }
    }
}

use test_field::TestField;

// Helper function to create test data matrix
fn create_test_data() -> Array2<TestField> {
    array![
        [TestField(1), TestField(2), TestField(3), TestField(4)],
        [TestField(5), TestField(6), TestField(0), TestField(1)],
        [TestField(2), TestField(3), TestField(4), TestField(5)],
        [TestField(6), TestField(0), TestField(1), TestField(2)]
    ]
}

// Helper function to create test G matrix (linear code)
fn create_test_g_matrix() -> Array2<TestField> {
    array![
        [TestField(1), TestField(0), TestField(0), TestField(0)],
        [TestField(0), TestField(1), TestField(0), TestField(0)],
        [TestField(0), TestField(0), TestField(1), TestField(0)],
        [TestField(0), TestField(0), TestField(0), TestField(1)],
        [TestField(2), TestField(3), TestField(1), TestField(4)],
        [TestField(5), TestField(1), TestField(6), TestField(2)]
    ]
}

// Helper function to create test G' matrix
fn create_test_g_prime_matrix() -> Array2<TestField> {
    array![
        [TestField(1), TestField(0), TestField(0), TestField(0)],
        [TestField(0), TestField(1), TestField(0), TestField(0)],
        [TestField(0), TestField(0), TestField(1), TestField(0)],
        [TestField(0), TestField(0), TestField(0), TestField(1)],
        [TestField(1), TestField(2), TestField(3), TestField(4)],
        [TestField(5), TestField(6), TestField(0), TestField(1)]
    ]
}

#[test]
fn test_complete_polynomial_commitment() {
    // Create a deterministic RNG for testing
    let seed = [0u8; 32];
    let mut rng = StdRng::from_seed(seed);
    
    let commitment = PolynomialCommitment::<TestField>::new();
    
    // Create test matrices
    let data = create_test_data();
    let g = create_test_g_matrix();
    let g_prime_t = create_test_g_prime_matrix().t().to_owned();
    
    // Create the polynomial commitment
    let output = commitment.commit(
        &data.view(),
        &g.view(),
        &g_prime_t.view(),
        &mut rng
    ).unwrap();
    
    // Verify the output contains the expected components
    assert!(!output.encoder_output.g_r.is_empty());
    assert!(!output.encoder_output.g_r_prime.is_empty());
    assert!(!output.encoder_output.yr.is_empty());
    assert!(!output.encoder_output.wr_prime.is_empty());
    
    // Open the polynomial at the same point used for commitment
    let evaluation = commitment.open_at_point(
        &output,
        &output.r_values,
        &output.r_prime_values
    ).unwrap();
    
    // Verify using a subset of rows and columns
    // For testing, we'll use all rows and columns, but in practice
    // one would use a random subset to reduce verification cost
    let y_rows = output.encoder_output.z.clone();
    let w_cols = output.encoder_output.z.t().to_owned();
    
    let verification_result = commitment.verify(
        &output.encoder_output.rows_commitment,
        &output.encoder_output.cols_commitment,
        &output.encoder_output.g_r,
        &output.encoder_output.g_r_prime,
        &output.encoder_output.yr,
        &output.encoder_output.wr_prime,
        &y_rows.view(),
        &w_cols.view(),
        &g.view(),
        &g_prime_t.t().view()
    ).unwrap();
    
    // Verification should succeed
    assert!(verification_result);
    
    // Now, let's test with incorrect values
    let mut incorrect_yr = output.encoder_output.yr.clone();
    incorrect_yr[0] = TestField(incorrect_yr[0].0 + 1); // Modify one element
    
    let incorrect_verification = commitment.verify(
        &output.encoder_output.rows_commitment,
        &output.encoder_output.cols_commitment,
        &output.encoder_output.g_r,
        &output.encoder_output.g_r_prime,
        &incorrect_yr,
        &output.encoder_output.wr_prime,
        &y_rows.view(),
        &w_cols.view(),
        &g.view(),
        &g_prime_t.t().view()
    ).unwrap();
    
    // Verification should fail with incorrect yr
    assert!(!incorrect_verification);
}

#[test]
fn test_subset_polynomial_commitment() {
    // Create a deterministic RNG for testing
    let seed = [0u8; 32];
    let mut rng = StdRng::from_seed(seed);
    
    let commitment = PolynomialCommitment::<TestField>::new();
    
    // Create test matrices
    let data = create_test_data();
    let g = create_test_g_matrix();
    let g_prime_t = create_test_g_prime_matrix().t().to_owned();
    
    // Create a subset of the data (first two rows)
    let subset_data = data.slice(ndarray::s![0..2, ..]).to_owned();
    let subset_indices = vec![0, 1];
    
    // Create the subset polynomial commitment
    let subset_output = commitment.commit_subset(
        &subset_data.view(),
        &subset_indices,
        &g.slice(ndarray::s![0..4, ..]).to_owned().view(), // Use appropriate subset of G
        &g_prime_t.view(),
        &mut rng
    ).unwrap();
    
    // Verify the output contains the expected components
    assert!(!subset_output.encoder_output.g_r.is_empty());
    assert!(!subset_output.encoder_output.g_r_prime.is_empty());
    assert!(!subset_output.encoder_output.yr.is_empty());
    assert!(!subset_output.encoder_output.wr_prime.is_empty());
    
    // This test demonstrates that we can create polynomial commitments
    // over a subset of the data, as described in section 5 of the paper
    println!("Successfully created subset polynomial commitment");
}
