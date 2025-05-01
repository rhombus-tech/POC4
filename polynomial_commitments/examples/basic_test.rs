//! Basic standalone test for polynomial commitment scheme

use ff::Field;
use ndarray::{array, Array2, ArrayView2};
use rand::thread_rng;
use std::fmt::Debug;
use std::marker::PhantomData;
use polynomial_commitments::tensor::TensorEncoder;

// Use a small finite field for testing
// We're implementing a simple GF(7) field for testing purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Gf7(pub u8);

impl Gf7 {
    fn new(value: u8) -> Self {
        Self(value % 7)
    }
}

// Required traits for Field implementation
impl std::ops::Add for Gf7 {
    type Output = Self;
    
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.0.wrapping_add(rhs.0))
    }
}

impl<'a> std::ops::Add<&'a Gf7> for Gf7 {
    type Output = Self;
    
    fn add(self, rhs: &'a Gf7) -> Self::Output {
        Self::new(self.0.wrapping_add(rhs.0))
    }
}

impl std::ops::AddAssign for Gf7 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<'a> std::ops::AddAssign<&'a Gf7> for Gf7 {
    fn add_assign(&mut self, rhs: &'a Gf7) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub for Gf7 {
    type Output = Self;
    
    fn sub(self, rhs: Self) -> Self::Output {
        let result = if self.0 >= rhs.0 {
            self.0 - rhs.0
        } else {
            7 + self.0 - rhs.0
        };
        Self(result % 7)
    }
}

impl<'a> std::ops::Sub<&'a Gf7> for Gf7 {
    type Output = Self;
    
    fn sub(self, rhs: &'a Gf7) -> Self::Output {
        let result = if self.0 >= rhs.0 {
            self.0 - rhs.0
        } else {
            7 + self.0 - rhs.0
        };
        Self(result % 7)
    }
}

impl std::ops::SubAssign for Gf7 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl<'a> std::ops::SubAssign<&'a Gf7> for Gf7 {
    fn sub_assign(&mut self, rhs: &'a Gf7) {
        *self = *self - rhs;
    }
}

impl std::ops::Mul for Gf7 {
    type Output = Self;
    
    fn mul(self, rhs: Self) -> Self::Output {
        Self::new((self.0 as u16 * rhs.0 as u16) as u8)
    }
}

impl<'a> std::ops::Mul<&'a Gf7> for Gf7 {
    type Output = Self;
    
    fn mul(self, rhs: &'a Gf7) -> Self::Output {
        Self::new((self.0 as u16 * rhs.0 as u16) as u8)
    }
}

impl std::ops::MulAssign for Gf7 {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl<'a> std::ops::MulAssign<&'a Gf7> for Gf7 {
    fn mul_assign(&mut self, rhs: &'a Gf7) {
        *self = *self * rhs;
    }
}

impl std::ops::Neg for Gf7 {
    type Output = Self;
    
    fn neg(self) -> Self::Output {
        if self.0 == 0 {
            self
        } else {
            Self(7 - self.0)
        }
    }
}

// Implement required Field trait for ff crate
impl Field for Gf7 {
    const ZERO: Self = Self(0);
    const ONE: Self = Self(1);
    
    fn random(mut rng: impl rand::RngCore) -> Self {
        use rand::Rng;
        Self(rng.gen_range(0..7))
    }
    
    fn square(&self) -> Self {
        *self * *self
    }
    
    fn double(&self) -> Self {
        *self + *self
    }
    
    fn invert(&self) -> Option<Self> {
        if self.0 == 0 {
            None
        } else {
            // Brute force inversion for small field
            for i in 1..7 {
                if (self.0 as u16 * i as u16) % 7 == 1 {
                    return Some(Self(i));
                }
            }
            None
        }
    }
    
    fn sqrt(&self) -> Option<Self> {
        // Simplified for GF(7)
        Some(*self)
    }
}

// Implement Sum and Product traits
impl std::iter::Sum for Gf7 {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

impl<'a> std::iter::Sum<&'a Gf7> for Gf7 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + *b)
    }
}

impl std::iter::Product for Gf7 {
    fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ONE, |a, b| a * b)
    }
}

impl<'a> std::iter::Product<&'a Gf7> for Gf7 {
    fn product<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.fold(Self::ONE, |a, b| a * *b)
    }
}

// Test the basic tensor encoding functionality
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing polynomial commitment tensor operations with GF(7)");
    
    // Test data
    let data = array![
        [Gf7(1), Gf7(2)],
        [Gf7(3), Gf7(4)]
    ];
    
    // Generator matrices (using identity for simple verification)
    let g = array![
        [Gf7(1), Gf7(0)],
        [Gf7(0), Gf7(1)]
    ];
    
    let g_prime_t = array![
        [Gf7(1), Gf7(0)],
        [Gf7(0), Gf7(1)]
    ];
    
    // Create tensor encoder
    let encoder = TensorEncoder::<Gf7>::new();
    
    // Test tensor encoding Z = G*X*G'ᵀ
    let encoded = encoder.encode(&data.view(), &g.view(), &g_prime_t.view())?;
    println!("Original data: {:?}", data);
    println!("Encoded data (Z = G*X*G'ᵀ): {:?}", encoded);
    
    // With identity matrices, the encoding should equal the original data
    assert_eq!(encoded[[0, 0]], data[[0, 0]]);
    assert_eq!(encoded[[0, 1]], data[[0, 1]]);
    assert_eq!(encoded[[1, 0]], data[[1, 0]]);
    assert_eq!(encoded[[1, 1]], data[[1, 1]]);
    
    // Test randomness generation
    let r = vec![Gf7(2), Gf7(3)];
    let randomness = encoder.generate_randomness(2, &r)?;
    println!("Generated randomness from r={:?}: {:?}", r, randomness);
    
    // Test matrix-vector product for commitments
    let yr = encoder.matrix_vector_product(&data.view(), &randomness)?;
    println!("Matrix-vector product (X*g_r): {:?}", yr);
    
    println!("All tests passed!");
    Ok(())
}
