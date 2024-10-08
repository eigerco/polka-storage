//! This module implements the data type definitions needed for the Groth16 proof generation and
//! verification. Therefore, all types need to be `std` and `no-std` compatible.
//!
//! Some important types (`VerifyingKey`, `Proof`, and `PublicInputs`) need to be Substrate runtime
//! compatible, which means to implement Substrate defaults (i.e. `TypeInfo`, `Encode`, `Decode`
//! etc.).
//!
//! The very essential trait `pairing::Engine` doesn't bound basic implementations like `Eq`,
//! `PartialEq`, etc., but their typically used types on ascociated types do. So, either we can bound
//! that trait bound on all definitions in this crate, or we can manually implement these basic
//! trait definitions.
//!
//! The general idea is to switch from the `std`-dependent crate `blstrs` to `bls12_381` to make it
//! `no-std` compatible, we got inspired by the following example implementations:
//! * <https://github.com/VegeBun-csj/substrate-zk>
//! * <https://github.com/bright/zk-snarks-with-substrate>
//!
//! Note, that crate `bls12_381` is currently not audited.

// TODO: (@neutrinoks,20/10/2024): Check if we can fix to Bls12 (VerifyingKey<Bls12>).

mod std;
mod substrate;

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Debug;

pub use bls12_381::{Bls12, G1Affine, G2Affine, Scalar};
pub use pairing::{
    group::{
        ff::{Field, PrimeField},
        prime::PrimeCurveAffine,
        Curve,
    },
    Engine, MillerLoopResult, MultiMillerLoop,
};
use rand_xorshift::XorShiftRng;

/// The number of bytes when serialising a `G1Affine` by using `G1Affine::to_compressed()`.
const G1AFFINE_COMPRESSED_BYTES: usize = 48;
/// The number of bytes when serialising a `G1Affine` by using `G1Affine::to_compressed()`.
const G2AFFINE_COMPRESSED_BYTES: usize = 96;
/// The number of bytes when serialising a `G1Affine` by using `G1Affine::to_uncompressed()`.
const G1AFFINE_UNCOMPRESSED_BYTES: usize = 96;
/// The number of bytes when serialising a `G1Affine` by using `G1Affine::to_uncompressed()`.
const G2AFFINE_UNCOMPRESSED_BYTES: usize = 192;

/// This constant specifies the minimum number of bytes of a serialised `VerifyingKey`.
///
/// It gets calculated by the defined number of serialised bytes of `G1Affine` and `G2Affine` in
/// uncompressed format. An uncompressed serialised `G1Affine` are 96 bytes, an uncompressed
/// serialised `G2Affine` are 192 bytes. In `VerifyingKey` we have for sure 3 x `G1Affine` and
/// 3 x `G2Affine`. One serialised `u32` variable will be added.
/// That computes to: 3 x 96 + 3 * 192 + 4 = 868.
pub const VERIFYINGKEY_MIN_BYTES: usize =
    3 * G1AFFINE_UNCOMPRESSED_BYTES + 3 * G2AFFINE_UNCOMPRESSED_BYTES + 4;
/// This constant specifies the maximum number of bytes of a serialised `VerifyingKey`.
///
/// The maximum number of parameters in field `ic` is 40 because its depedency can be resolved to
/// possible sector sizes. This computes to: 3 * 96 + 3 * 192 + 4 + 40 * 96 = 4704.
pub const VERIFYINGKEY_MAX_BYTES: usize =
    43 * G1AFFINE_UNCOMPRESSED_BYTES + 3 * G2AFFINE_UNCOMPRESSED_BYTES + 4;

/// This constant specifies the number of bytes of a serialised `Proof`.
///
/// It gets calculated by the defined nubmer of compressed serialised bytes of `G1Affine` and
/// `G2Affine`. A compressed serialised `G1Affine` are 48 bytes, a compressed serialised `G2Affine`
/// are 96 bytes.
/// That computes to: 2 * 48 + 96 = 192.
pub const PROOF_BYTES: usize = 2 * G1AFFINE_COMPRESSED_BYTES + 1 * G2AFFINE_COMPRESSED_BYTES;

/// The Verifying-Key data type definition for a ZK-SNARK verification. This type definition is
/// `std`- and `no-std`-compatible, and Substrate-runtime-compatible as well.
///
/// References:
/// * <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/compound_proof.rs#L384>
/// * <https://github.com/filecoin-project/bellperson/blob/master/src/groth16/verifying_key.rs#L14-L39>
/// * <https://github.com/zkcrypto/bellman/blob/main/groth16/src/lib.rs#L103-L128>
#[derive(Clone, Debug, Eq)]
pub struct VerifyingKey<E: Engine> {
    /// Alpha in g1 for verifying and for creating A/C elements of proof.
    /// Never the point at infinity.
    pub alpha_g1: E::G1Affine,
    /// Beta in g1 and g2 for verifying and for creating B/C elements of proof.
    /// Never the point at infinity.
    pub beta_g1: E::G1Affine,
    /// Beta in g1 and g2 for verifying and for creating B/C elements of proof.
    /// Never the point at infinity.
    pub beta_g2: E::G2Affine,
    /// Gamma in g2 for verifying.
    /// Never the point at infinity.
    pub gamma_g2: E::G2Affine,
    /// Delta in g1 and g2 for verifying and proving, essentially the magic trapdoor that forces the
    /// prover to evaluate the C element of the proof with only components from the CRS.
    /// Never the point at infinity.
    pub delta_g1: E::G1Affine,
    /// Delta in g1 and g2 for verifying and proving, essentially the magic trapdoor that forces the
    /// prover to evaluate the C element of the proof with only components from the CRS.
    /// Never the point at infinity.
    pub delta_g2: E::G2Affine,
    /// Elements of the form (beta * u_i(tau) + alpha v_i(tau) + w_i(tau)) / gamma
    /// for all public inputs. Because all public inputs have a dummy constraint,
    /// this is the same size as the number of inputs and never contains points
    /// at infinity.
    pub ic: Vec<E::G1Affine>,
}

impl<E: Engine> PartialEq for VerifyingKey<E> {
    fn eq(&self, other: &Self) -> bool {
        self.alpha_g1 == other.alpha_g1
            && self.beta_g1 == other.beta_g1
            && self.beta_g2 == other.beta_g2
            && self.gamma_g2 == other.gamma_g2
            && self.delta_g1 == other.delta_g1
            && self.delta_g2 == other.delta_g2
            && self.ic == other.ic
    }
}

impl<E> VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    /// Serialises the `VerifiyingKey` into a byte stream and writes it to the given buffer.
    pub fn into_bytes(&self, buf: &mut [u8]) -> Result<(), IntoBytesError> {
        if buf.len() < self.serialised_bytes() {
            return Err(IntoBytesError::InsufficientBufferLength);
        }

        let mut idx = 0;

        idx = from_fixed_buffer(buf, idx, &self.alpha_g1.to_uncompressed());
        idx = from_fixed_buffer(buf, idx, &self.beta_g1.to_uncompressed());
        idx = from_fixed_buffer(buf, idx, &self.beta_g2.to_uncompressed());
        idx = from_fixed_buffer(buf, idx, &self.gamma_g2.to_uncompressed());
        idx = from_fixed_buffer(buf, idx, &self.delta_g1.to_uncompressed());
        idx = from_fixed_buffer(buf, idx, &self.delta_g2.to_uncompressed());
        idx = from_fixed_buffer(buf, idx, &(self.ic.len() as u32).to_be_bytes());
        for ic in &self.ic {
            idx = from_fixed_buffer(buf, idx, &ic.to_uncompressed());
        }

        Ok(())
    }

    /// Tries to deserialise a given byte stream into `VerifiyingKey`.
    pub fn from_bytes(bytes: &[u8]) -> Result<VerifyingKey<E>, FromBytesError> {
        // G1Affine::to_uncompressed() transforms it into 96 bytes.
        // G2Affine::to_uncompressed() transforms it into 192 bytes.
        if bytes.len() < VERIFYINGKEY_MIN_BYTES {
            return Err(FromBytesError::NumberOfSerialisedBytes);
        }

        let mut g1_chunk = [0u8; G1AFFINE_UNCOMPRESSED_BYTES];
        let mut g2_chunk = [0u8; G2AFFINE_UNCOMPRESSED_BYTES];
        let mut u32_chunk = [0u8; 4];
        let mut idx = 0;

        idx = to_fixed_buffer(&mut g1_chunk, idx, bytes);
        let alpha_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = to_fixed_buffer(&mut g1_chunk, idx, bytes);
        let beta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = to_fixed_buffer(&mut g2_chunk, idx, bytes);
        let beta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        idx = to_fixed_buffer(&mut g2_chunk, idx, bytes);
        let gamma_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        idx = to_fixed_buffer(&mut g1_chunk, idx, bytes);
        let delta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = to_fixed_buffer(&mut g2_chunk, idx, bytes);
        let delta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        idx = to_fixed_buffer(&mut u32_chunk, idx, bytes);
        let ic_len = u32::from_be_bytes(u32_chunk) as usize;
        if bytes.len() - idx != ic_len * G1AFFINE_UNCOMPRESSED_BYTES {
            return Err(FromBytesError::NumberOfSerialisedBytes);
        }

        let mut ic = Vec::<G1Affine>::new();
        while idx <= bytes.len() - G1AFFINE_UNCOMPRESSED_BYTES {
            idx = to_fixed_buffer(&mut g1_chunk, idx, bytes);
            ic.push(
                G1Affine::from_uncompressed(&g1_chunk)
                    .into_option()
                    .ok_or(FromBytesError::G1AffineConversion)?,
            );
        }

        Ok(VerifyingKey::<E> {
            alpha_g1,
            beta_g1,
            beta_g2,
            gamma_g2,
            delta_g1,
            delta_g2,
            ic,
        })
    }

    /// Method returns the number of bytes when serialised.
    pub fn serialised_bytes(&self) -> usize {
        VERIFYINGKEY_MIN_BYTES + self.ic.len() * G1AFFINE_UNCOMPRESSED_BYTES
    }

    /// Method generates a `VerifyingKey` with random numbers.
    pub fn random(rng: &mut XorShiftRng) -> VerifyingKey<E> {
        VerifyingKey::<E> {
            alpha_g1: rand_g1affine(rng),
            beta_g1: rand_g1affine(rng),
            beta_g2: rand_g2affine(rng),
            gamma_g2: rand_g2affine(rng),
            delta_g1: rand_g1affine(rng),
            delta_g2: rand_g2affine(rng),
            ic: alloc::vec![rand_g1affine(rng), rand_g1affine(rng)],
        }
    }
}

/// The Proof type definition for a ZK-SNARK verification. This type definition is `std`- and
/// `no-std`-compatible, and Substrate-runtime-compatible as well.
///
/// References:
/// - <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/multi_proof.rs#L10>
/// - <https://github.com/filecoin-project/bellperson/blob/1264fa12bc2b79cdfbb1f5764349c9a22f2d8ea3/src/groth16/proof.rs#L14>
/// - <https://github.com/zkcrypto/bellman/blob/9bb30a7bd261f2aa62840b80ed6750c622bebec3/src/groth16/mod.rs#L27>
#[derive(Clone, Debug, Eq)]
pub struct Proof<E: Engine> {
    pub a: E::G1Affine,
    pub b: E::G2Affine,
    pub c: E::G1Affine,
}

impl<E: Engine> PartialEq for Proof<E> {
    fn eq(&self, other: &Self) -> bool {
        self.a == other.a && self.b == other.b && self.c == other.c
    }
}

impl<E> Proof<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    /// Serialises the `Proof` into a byte stream and writes it to the given buffer.
    pub fn into_bytes(&self, buf: &mut [u8]) -> Result<(), IntoBytesError> {
        if buf.len() < PROOF_BYTES {
            return Err(IntoBytesError::InsufficientBufferLength);
        }

        let mut idx = 0;

        idx = from_fixed_buffer(buf, idx, &self.a.to_compressed());
        idx = from_fixed_buffer(buf, idx, &self.b.to_compressed());
        from_fixed_buffer(buf, idx, &self.c.to_compressed());

        Ok(())
    }

    /// Tries to deserialise a given byte stream into `Proof`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Proof<E>, FromBytesError> {
        // G1Affine::to_uncompressed() transforms it into 96 bytes.
        // G2Affine::to_uncompressed() transforms it into 192 bytes.
        if bytes.len() < PROOF_BYTES {
            return Err(FromBytesError::NumberOfSerialisedBytes);
        }

        let mut g1_chunk = [0u8; G1AFFINE_COMPRESSED_BYTES];
        let mut g2_chunk = [0u8; G2AFFINE_COMPRESSED_BYTES];
        let mut idx = 0;

        idx = to_fixed_buffer(&mut g1_chunk, idx, bytes);
        let a = G1Affine::from_compressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = to_fixed_buffer(&mut g2_chunk, idx, bytes);
        let b = G2Affine::from_compressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        to_fixed_buffer(&mut g1_chunk, idx, bytes);
        let c = G1Affine::from_compressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        Ok(Proof::<E> { a, b, c })
    }

    /// Method returns the number of bytes when serialised.
    pub const fn serialised_bytes() -> usize {
        PROOF_BYTES
    }

    /// Method generates a `Proof` with random numbers.
    pub fn random(rng: &mut XorShiftRng) -> Proof<E> {
        Proof::<E> {
            a: rand_g1affine(rng),
            b: rand_g2affine(rng),
            c: rand_g1affine(rng),
        }
    }
}

/// Error type on serialisation of the above defined types. They can occur on deserialisation of a
/// byte stream into the defined data type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "substrate",
    derive(::codec::Decode, ::codec::Encode, ::scale_info::TypeInfo)
)]
pub enum IntoBytesError {
    /// The given buffer is not large enough when using `into_bytes()`.
    InsufficientBufferLength,
}

/// Error type on deserialisation of the above defined types. They can occur on deserialisation of a
/// byte stream into the defined data type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "substrate",
    derive(::codec::Decode, ::codec::Encode, ::scale_info::TypeInfo)
)]
pub enum FromBytesError {
    /// The given number of bytes is not valid.
    NumberOfSerialisedBytes,
    /// A conversion error when using `G1Affine::from_uncompressed()`.
    G1AffineConversion,
    /// A conversion error when using `G2Affine::from_uncompressed()`.
    G2AffineConversion,
    /// A conversion error when using 'Scalar::from_uncompressed()`.
    ScalarConversion,
}

impl AsRef<str> for FromBytesError {
    fn as_ref(&self) -> &str {
        self.as_static_str()
    }
}

impl FromBytesError {
    pub fn as_static_str(&self) -> &'static str {
        match self {
            FromBytesError::NumberOfSerialisedBytes => "NumberOfSerialisedBytes",
            FromBytesError::G1AffineConversion => "G1AffineConversion",
            FromBytesError::G2AffineConversion => "G2AffineConversion",
            FromBytesError::ScalarConversion => "ScalarConversion",
        }
    }
}

/// Locally used method to copy bytes to a fixed sized buffers, step by step.
fn to_fixed_buffer(buffer: &mut [u8], idx: usize, bytes: &[u8]) -> usize {
    let len = buffer.len();
    let end = idx + len;
    buffer.copy_from_slice(&bytes[idx..end]);
    end
}

/// Locally used method to copy bytes from a fixed sized buffers, step by step.
fn from_fixed_buffer(bytes: &mut [u8], idx: usize, buffer: &[u8]) -> usize {
    let len = buffer.len();
    let end = idx + len;
    bytes[idx..end].copy_from_slice(&buffer);
    end
}

fn rand_g1affine(rng: &mut XorShiftRng) -> G1Affine {
    (G1Affine::generator() * Scalar::random(rng)).into()
}

fn rand_g2affine(rng: &mut XorShiftRng) -> G2Affine {
    (G2Affine::generator() * Scalar::random(rng)).into()
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    /// Locally used test seed for random number generation.
    pub(crate) const TEST_SEED: [u8; 16] = [
        0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc,
        0xe5,
    ];

    /// This test is about the serialisation and deserialisation of `VerifyingKey`.
    #[test]
    fn verifyingkey_into_bytes_and_from_bytes() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = VerifyingKey::<Bls12>::random(&mut rng);
        let mut vkey_bytes = vec![0u8; vkey.serialised_bytes()];
        vkey.clone()
            .into_bytes(&mut vkey_bytes.as_mut_slice())
            .unwrap();
        assert_eq!(
            vkey,
            VerifyingKey::<Bls12>::from_bytes(&vkey_bytes.as_slice()).unwrap()
        );
    }

    /// This test is about the serialisation and deserialisation of `Proof`.
    #[test]
    fn proof_into_bytes_and_from_bytes() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let proof = Proof::<Bls12>::random(&mut rng);
        let mut proof_bytes = vec![0u8; PROOF_BYTES];
        proof
            .clone()
            .into_bytes(&mut proof_bytes.as_mut_slice())
            .unwrap();
        assert_eq!(
            proof,
            Proof::<Bls12>::from_bytes(&proof_bytes.as_slice()).unwrap()
        );
    }

    #[test]
    fn test_random_numbers_are_not_the_same() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let mut last = rand_g1affine(&mut rng);
        let mut next = rand_g1affine(&mut rng);
        assert_ne!(last, next);
        last = next;
        next = rand_g1affine(&mut rng);
        assert_ne!(last, next);
        last = next;
        next = rand_g1affine(&mut rng);
        assert_ne!(last, next);
    }
}
