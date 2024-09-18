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

// TODO: (395,@neutrinoks,20/10/2024): Check if we can fix to Bls12 (VerifyingKey<Bls12>).

extern crate alloc;

#[cfg(not(feature = "substrate"))]
use alloc::vec::Vec;
use core::fmt::Debug;

#[cfg(feature = "std")]
use bellperson::groth16 as bp_g16;
pub use bls12_381::{Bls12, G1Affine, G2Affine, Scalar};
#[cfg(feature = "substrate")]
use codec::{Decode, Encode, Error as CodecError, Input, Output};
pub use pairing::{
    group::{ff::PrimeField, prime::PrimeCurveAffine, Curve},
    Engine, MultiMillerLoop,
};
#[cfg(feature = "substrate")]
#[allow(unused_imports)]
use sp_std::{vec, vec::Vec};

/// This constant specifies the minimum number of bytes of a serialized `VerifyingKey`.
///
/// It gets calculated by the defined number of serialised bytes of `G1Affine` and `G2Affine`. A
/// serialised `G1Affine` are 96 bytes, a serialised `G2Affine` are 192 bytes. In `VerifyingKey` we
/// have for sure 3 x `G1Affine` and 3 x `G2Affine`. One serialised `u32` variable will be added.
/// That computes to: 3 x 96 + 3 * 192 + 4 = 868.
pub const VERIFYINGKEY_MIN_BYTES: usize = 868;

/// The number of bytes when serialising a `G1Affine` by using `G1Affine::to_uncompressed()`.
const G1AFFINE_BYTES: usize = 96;
/// The number of bytes when serialising a `G1Affine` by using `G1Affine::to_uncompressed()`.
const G2AFFINE_BYTES: usize = 192;

/// The Verifying-Key data type definition for a ZK-SNARK verification. This type definition is
/// `std`- and `no-std`-compatible, and Substrate-runtime-compatible as well.
///
/// References:
/// * <https://github.com/eigerco/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/compound_proof.rs#L384>
/// * <https://github.com/filecoin-project/bellperson/blob/master/src/groth16/verifying_key.rs#L14-L39>
/// * <https://github.com/zkcrypto/bellman/blob/main/groth16/src/lib.rs#L103-L128>
#[derive(Clone, Debug, Eq)]
#[cfg_attr(feature = "substrate", derive(Default, ::scale_info::TypeInfo))]
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

#[cfg(feature = "substrate")]
impl<E> Decode for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        // TODO(@neutrinoks,#395,20/09/2024): check again needed buffer size.
        // We don't know how many `ic` values will be passed.
        // 2784 == 20 * 96 + 864 | see notes above.
        let mut buffer = [0u8; 2784];
        let Some(n_bytes) = input.remaining_len()? else {
            return Err(CodecError::from("unable to get remaining_len"));
        };
        input.read(&mut buffer[..n_bytes])?;
        VerifyingKey::<E>::from_bytes(&buffer[..n_bytes])
            .map_err(|e| codec::Error::from(e.as_static_str()))
    }
}

#[cfg(feature = "substrate")]
impl<E> Encode for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn size_hint(&self) -> usize {
        self.serialised_bytes()
    }

    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        dest.write(&self.alpha_g1.to_uncompressed()[..]);
        dest.write(&self.beta_g1.to_uncompressed()[..]);
        dest.write(&self.beta_g2.to_uncompressed()[..]);
        dest.write(&self.gamma_g2.to_uncompressed()[..]);
        dest.write(&self.delta_g1.to_uncompressed()[..]);
        dest.write(&self.delta_g2.to_uncompressed()[..]);
        dest.write(&(self.ic.len() as u32).to_be_bytes()[..]);
        for ic in &self.ic {
            dest.write(&ic.to_uncompressed()[..]);
        }
    }

    fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
        let mut buffer = ByteBuffer::new();
        self.encode_to(&mut buffer);
        f(buffer.as_slice())
    }
}

#[cfg(feature = "std")]
impl<E> TryFrom<bp_g16::VerifyingKey<blstrs::Bls12>> for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    type Error = FromBytesError;

    fn try_from(vkey: bp_g16::VerifyingKey<blstrs::Bls12>) -> Result<Self, Self::Error> {
        let mut ic = Vec::<G1Affine>::new();
        for i in &vkey.ic {
            ic.push(g1affine(i)?);
        }
        Ok(VerifyingKey::<E> {
            alpha_g1: g1affine(&vkey.alpha_g1)?,
            beta_g1: g1affine(&vkey.beta_g1)?,
            beta_g2: g2affine(&vkey.beta_g2)?,
            gamma_g2: g2affine(&vkey.gamma_g2)?,
            delta_g1: g1affine(&vkey.delta_g1)?,
            delta_g2: g2affine(&vkey.delta_g2)?,
            ic,
        })
    }
}

impl<E> VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    /// Serialises the `VerifiyingKey` into a byte stream.
    pub fn into_bytes(&self, buf: &mut [u8]) -> Result<(), IntoBytesError> {
        if buf.len() < self.serialised_bytes() {
            return Err(IntoBytesError::InsufficientBufferLength);
        }

        let mut idx = 0;

        idx = copy_from_buffer(buf, idx, &self.alpha_g1.to_uncompressed());
        idx = copy_from_buffer(buf, idx, &self.beta_g1.to_uncompressed());
        idx = copy_from_buffer(buf, idx, &self.beta_g2.to_uncompressed());
        idx = copy_from_buffer(buf, idx, &self.gamma_g2.to_uncompressed());
        idx = copy_from_buffer(buf, idx, &self.delta_g1.to_uncompressed());
        idx = copy_from_buffer(buf, idx, &self.delta_g2.to_uncompressed());
        idx = copy_from_buffer(buf, idx, &(self.ic.len() as u32).to_be_bytes());
        for ic in &self.ic {
            idx = copy_from_buffer(buf, idx, &ic.to_uncompressed());
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

        let mut g1_chunk = [0u8; G1AFFINE_BYTES];
        let mut g2_chunk = [0u8; G2AFFINE_BYTES];
        let mut u32_chunk = [0u8; 4];
        let mut idx = 0;

        idx = copy_to_buffer(&mut g1_chunk, idx, bytes);
        let alpha_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = copy_to_buffer(&mut g1_chunk, idx, bytes);
        let beta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = copy_to_buffer(&mut g2_chunk, idx, bytes);
        let beta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        idx = copy_to_buffer(&mut g2_chunk, idx, bytes);
        let gamma_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        idx = copy_to_buffer(&mut g1_chunk, idx, bytes);
        let delta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(FromBytesError::G1AffineConversion)?;

        idx = copy_to_buffer(&mut g2_chunk, idx, bytes);
        let delta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(FromBytesError::G2AffineConversion)?;

        idx = copy_to_buffer(&mut u32_chunk, idx, bytes);
        let ic_len = u32::from_be_bytes(u32_chunk) as usize;
        if bytes.len() - idx != ic_len * G1AFFINE_BYTES {
            return Err(FromBytesError::NumberOfSerialisedBytes);
        }

        let mut ic = Vec::<G1Affine>::new();
        while idx <= bytes.len() - G1AFFINE_BYTES {
            idx = copy_to_buffer(&mut g1_chunk, idx, bytes);
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
        VERIFYINGKEY_MIN_BYTES + self.ic.len() * G1AFFINE_BYTES
    }
}

/// Error type on serialisation of the above defined types. They can occur on deserialisation of a
/// byte stream into the defined data type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "substrate", derive(Decode, Encode, ::scale_info::TypeInfo))]
pub enum IntoBytesError {
    /// The given buffer is not large enough when using `into_bytes()`.
    InsufficientBufferLength,
}

/// Error type on deserialisation of the above defined types. They can occur on deserialisation of a
/// byte stream into the defined data type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "substrate", derive(Decode, Encode, ::scale_info::TypeInfo))]
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

/// Locally used method to copy bytes to fixed sized buffers, step by step.
fn copy_to_buffer(buffer: &mut [u8], idx: usize, bytes: &[u8]) -> usize {
    let len = buffer.len();
    let end = idx + len;
    buffer.copy_from_slice(&bytes[idx..end]);
    end
}

/// Locally used method to copy bytes to fixed sized buffers, step by step.
fn copy_from_buffer(bytes: &mut [u8], idx: usize, buffer: &[u8]) -> usize {
    let len = buffer.len();
    let end = idx + len;
    bytes[idx..end].copy_from_slice(&buffer);
    end
}

/// Helper definition that implements `codec::Output`.
pub struct ByteBuffer(Vec<u8>, usize);

#[cfg(feature = "substrate")]
impl Output for ByteBuffer {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes.iter() {
            self.0.push(*b);
        }
    }
}

#[cfg(feature = "substrate")]
impl Input for ByteBuffer {
    fn remaining_len(&mut self) -> Result<Option<usize>, CodecError> {
        Ok(Some(self.bytes_to_read()))
    }

    fn read(&mut self, into: &mut [u8]) -> Result<(), CodecError> {
        let max = self.bytes_to_read();
        let n = core::cmp::min(into.len(), max);
        self.1 = copy_to_buffer(&mut into[..n], self.1, &self.0[..]);
        Ok(())
    }
}

#[cfg(feature = "std")]
impl std::io::Read for ByteBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = std::cmp::min(self.0.len(), buf.len());
        buf.copy_from_slice(&self.0[self.1..self.1 + n]);
        self.1 += n;
        Ok(n)
    }
}

#[cfg(feature = "std")]
impl std::io::Write for ByteBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        assert!(buf.len() > 0, "{}", buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, feature = "substrate"))]
impl From<Vec<u8>> for ByteBuffer {
    fn from(vec: Vec<u8>) -> ByteBuffer {
        ByteBuffer(vec, 0)
    }
}

impl ByteBuffer {
    /// New type pattern. Initialises an empty buffer.
    pub fn new() -> ByteBuffer {
        ByteBuffer(Vec::<u8>::new(), 0)
    }

    /// New type patterns with pre-initialised given size.
    pub fn new_with_size(len: usize) -> ByteBuffer {
        let mut vec = Vec::<u8>::with_capacity(len);
        for _ in 0..len {
            vec.push(0);
        }
        ByteBuffer(vec, 0)
    }

    /// Returns the internal buffer as a non-mutable slice.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Returns the internal buffer as a mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }

    /// When reading from the buffer, this method returns how many bytes are left to be read.
    pub fn bytes_to_read(&self) -> usize {
        self.0.len() - self.1
    }
}

/// Method transforms a `blstrs::G1Affine` into a `bls12_381::G1Affine`.
#[cfg(feature = "std")]
fn g1affine(affine: &blstrs::G1Affine) -> Result<G1Affine, FromBytesError> {
    G1Affine::from_uncompressed(&affine.to_uncompressed())
        .into_option()
        .ok_or(FromBytesError::G1AffineConversion)
}

/// Method transforms a `blstrs::G2Affine` into a `bls12_381::G2Affine`.
#[cfg(feature = "std")]
fn g2affine(affine: &blstrs::G2Affine) -> Result<G2Affine, FromBytesError> {
    G2Affine::from_uncompressed(&affine.to_uncompressed())
        .into_option()
        .ok_or(FromBytesError::G2AffineConversion)
}

/// Method transforms a `blstrs::Scalar` into a `bls12_381::Scalar`.
// TODO: (395,@neutrinoks,20/10/2024): Remove allow dead_code.
#[allow(dead_code)]
#[cfg(feature = "std")]
fn scalar(scalar: &blstrs::Scalar) -> Result<Scalar, FromBytesError> {
    Scalar::from_bytes(&scalar.to_bytes_le())
        .into_option()
        .ok_or(FromBytesError::ScalarConversion)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "substrate")]
    use codec::{Decode, Encode};

    use super::*;

    fn random_verifying_key() -> VerifyingKey<Bls12> {
        VerifyingKey::<Bls12> {
            alpha_g1: G1Affine::generator(),
            beta_g1: G1Affine::generator(),
            beta_g2: G2Affine::generator(),
            gamma_g2: G2Affine::generator(),
            delta_g1: G1Affine::generator(),
            delta_g2: G2Affine::generator(),
            ic: vec![G1Affine::generator(), G1Affine::generator()],
        }
    }

    #[cfg(feature = "std")]
    fn random_bellperson_verifying_key() -> bp_g16::VerifyingKey<blstrs::Bls12> {
        bp_g16::VerifyingKey::<blstrs::Bls12> {
            alpha_g1: blstrs::G1Affine::generator(),
            beta_g1: blstrs::G1Affine::generator(),
            beta_g2: blstrs::G2Affine::generator(),
            gamma_g2: blstrs::G2Affine::generator(),
            delta_g1: blstrs::G1Affine::generator(),
            delta_g2: blstrs::G2Affine::generator(),
            ic: vec![
                blstrs::G1Affine::generator(),
                blstrs::G1Affine::generator(),
                blstrs::G1Affine::generator(),
            ],
        }
    }

    /// This test is about the serialisation and deserialisation of `VerifyingKey`.
    #[test]
    fn verifying_key_into_bytes_and_from_bytes() {
        let vkey = random_verifying_key();
        let mut vkey_bytes = ByteBuffer::new_with_size(vkey.serialised_bytes());
        vkey.clone()
            .into_bytes(&mut vkey_bytes.as_mut_slice())
            .unwrap();
        assert_eq!(
            vkey,
            VerifyingKey::<Bls12>::from_bytes(&vkey_bytes.as_slice()).unwrap()
        );
    }

    /// This is a smoke test of the `codec::Encode` and `codec::Decode` implementation.
    #[cfg(feature = "substrate")]
    #[test]
    fn verifying_key_encode_decode() {
        let vkey = random_verifying_key();
        let vkey_bytes = vkey.encode();
        let mut output = ByteBuffer::from(vkey_bytes);
        assert_eq!(
            vkey,
            VerifyingKey::decode(&mut output).expect("VerifyingKey::decode failed")
        );
    }

    /// This test is about testing the deserialisation of `VerifyingKey` with bytes that have been
    /// serialised from `bellperson::VerifyingKey`.
    #[cfg(feature = "std")]
    #[test]
    fn verifying_key_from_bytes_from_bellperson() {
        // Generate a verifying key with bellperson crate.
        let bp_vkey = random_bellperson_verifying_key();
        // Smoke test about converting it directly to our implemmentation.
        let vkey =
            VerifyingKey::<Bls12>::try_from(bp_vkey.clone()).expect("expect VerifiyingKey::from");
        // Compare each single fields by their bytes to make sure conversion was correct.
        assert_eq!(
            bp_vkey.alpha_g1.to_uncompressed(),
            vkey.alpha_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g1.to_uncompressed(),
            vkey.beta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g2.to_uncompressed(),
            vkey.beta_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.gamma_g2.to_uncompressed(),
            vkey.gamma_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g1.to_uncompressed(),
            vkey.delta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g2.to_uncompressed(),
            vkey.delta_g2.to_uncompressed()
        );
        assert_eq!(bp_vkey.ic.len(), vkey.ic.len());
        for i in 0..bp_vkey.ic.len() {
            assert_eq!(
                bp_vkey.ic[i].to_uncompressed(),
                vkey.ic[i].to_uncompressed()
            );
        }
    }

    // #[ignore = "to be fixed, errors on VerifyingKey::write()"]
    #[cfg(feature = "std")]
    #[test]
    fn verifyingkey_serialise_and_deserialise_direct_bellperson() {
        // Generate a verifying key with bellperson crate.
        let bp_vkey = random_bellperson_verifying_key();
        // Serialise it by using its `Read` implementation.
        let bytes = VERIFYINGKEY_MIN_BYTES + bp_vkey.ic.len() * G1AFFINE_BYTES;
        let mut bytes = ByteBuffer::new_with_size(bytes);
        bp_vkey.write(bytes.0.as_mut_slice()).unwrap();
        // Try to deserialise it by using `VerifyingKey::from_bytes()`.
        let vkey = VerifyingKey::<Bls12>::from_bytes(bytes.as_slice()).unwrap();
        // Compare their values.
        assert_eq!(
            bp_vkey.alpha_g1.to_uncompressed(),
            vkey.alpha_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g1.to_uncompressed(),
            vkey.beta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g2.to_uncompressed(),
            vkey.beta_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.gamma_g2.to_uncompressed(),
            vkey.gamma_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g1.to_uncompressed(),
            vkey.delta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g2.to_uncompressed(),
            vkey.delta_g2.to_uncompressed()
        );
        assert_eq!(bp_vkey.ic.len(), vkey.ic.len());
        for i in 0..bp_vkey.ic.len() {
            assert_eq!(
                bp_vkey.ic[i].to_uncompressed(),
                vkey.ic[i].to_uncompressed()
            );
        }
        // Serialise our implementation as well by using `VerifyingKey::into_bytes()'.
        let mut bytes = ByteBuffer::new_with_size(vkey.serialised_bytes());
        vkey.into_bytes(bytes.as_mut_slice()).unwrap();
        // Deserialise bytes to bellperson's VerifyingKey by using its `Write` implementation.
        let bp_vkey_result = bp_g16::VerifyingKey::<blstrs::Bls12>::read(bytes).unwrap();
        // Compare initial struct with this one.
        assert_eq!(bp_vkey, bp_vkey_result);
    }
}
