//! Proof related datatype definitions.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::ops::Neg;

pub use bls12_381::{Bls12, G1Affine, G2Affine};
use codec::{Decode, Encode};
pub use pairing::{
    group::{ff::PrimeField, prime::PrimeCurveAffine, Curve},
    Engine, MultiMillerLoop,
};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

/// This constant specifies the number of bytes of a serialized `Proof`.
pub const PROOF_BYTES: usize = 384;
/// This constant specifies the number of bytes of a serialized `Proof`.
pub const VERIFYINGKEY_MIN_BYTES: usize = 1056;

/// For more information on this definition check out the `bellperson`'s definition.
#[derive(Clone, Decode, Default, Encode, Eq, TypeInfo)]
pub struct Proof<E: Engine> {
    pub a: E::G1Affine,
    pub b: E::G2Affine,
    pub c: E::G1Affine,
}

impl<E: Engine> core::fmt::Debug for Proof<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "")
    }
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
    /// Turns the proof into `Vec<u8>`.
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::<u8>::new();

        bytes.append(&mut self.a.to_uncompressed().to_vec());
        bytes.append(&mut self.b.to_uncompressed().to_vec());
        bytes.append(&mut self.c.to_uncompressed().to_vec());

        bytes
    }

    /// Tries to create a `Proof` from given bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Proof<E>, ()> {
        if bytes.len() != PROOF_BYTES {
            return Err(());
        }

        let mut g1_chunk = [0u8; 96];
        let mut g2_chunk = [0u8; 192];
        let mut idx = 0;

        idx = copy_next_bytes(&mut g1_chunk, idx, bytes);
        let a = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(())?;

        idx = copy_next_bytes(&mut g2_chunk, idx, bytes);
        let b = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(())?;

        copy_next_bytes(&mut g1_chunk, idx, bytes);
        let c = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(())?;

        Ok(Proof::<E> { a, b, c })
    }
}

/// For more information on this definition check out the `bellperson`'s definition.
#[derive(Clone, Decode, Default, Encode, Eq, TypeInfo)]
pub struct VerifyingKey<E: Engine> {
    pub alpha_g1: E::G1Affine,
    pub beta_g1: E::G1Affine,
    pub beta_g2: E::G2Affine,
    pub gamma_g2: E::G2Affine,
    pub delta_g1: E::G1Affine,
    pub delta_g2: E::G2Affine,
    pub ic: Vec<E::G1Affine>,
}

impl<E: Engine> core::fmt::Debug for VerifyingKey<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "")
    }
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
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::<u8>::new();

        bytes.append(&mut self.alpha_g1.to_uncompressed().to_vec());
        bytes.append(&mut self.beta_g1.to_uncompressed().to_vec());
        bytes.append(&mut self.beta_g2.to_uncompressed().to_vec());
        bytes.append(&mut self.gamma_g2.to_uncompressed().to_vec());
        bytes.append(&mut self.delta_g1.to_uncompressed().to_vec());
        bytes.append(&mut self.delta_g2.to_uncompressed().to_vec());
        for ic in self.ic {
            bytes.append(&mut ic.to_uncompressed().to_vec());
        }

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<VerifyingKey<E>, ()> {
        if bytes.len() < VERIFYINGKEY_MIN_BYTES {
            return Err(());
        }
        if (bytes.len() - VERIFYINGKEY_MIN_BYTES) % 96 != 0 {
            return Err(());
        }

        let mut g1_chunk = [0u8; 96];
        let mut g2_chunk = [0u8; 192];
        let mut idx = 0;

        idx = copy_next_bytes(&mut g1_chunk, idx, bytes);
        let alpha_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(())?;

        idx = copy_next_bytes(&mut g1_chunk, idx, bytes);
        let beta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(())?;

        idx = copy_next_bytes(&mut g2_chunk, idx, bytes);
        let beta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(())?;

        idx = copy_next_bytes(&mut g2_chunk, idx, bytes);
        let gamma_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(())?;

        idx = copy_next_bytes(&mut g1_chunk, idx, bytes);
        let delta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(())?;

        idx = copy_next_bytes(&mut g2_chunk, idx, bytes);
        let delta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(())?;

        let mut ic = Vec::<G1Affine>::new();
        while idx <= bytes.len() - 96 {
            idx = copy_next_bytes(&mut g1_chunk, idx, bytes);
            ic.push(
                G1Affine::from_uncompressed(&g1_chunk)
                    .into_option()
                    .ok_or(())?,
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
}

/// TODO
#[derive(Clone, Decode, Default, Encode)] // Eq
pub struct PreparedVerifyingKey<E: MultiMillerLoop> {
    pub alpha_g1_beta_g2: E::Gt,
    pub neg_gamma_g2: E::G2Prepared,
    pub neg_delta_g2: E::G2Prepared,
    pub ic: Vec<E::G1Affine>,
}

// impl<E: MultiMillerLoop> PartialEq for PreparedVerifyingKey<E> {
//     fn eq(&self, other: &Self) -> bool {
//         if self.alpha_g1_beta_g2 != other.alpha_g1_beta_g2 ||
//             // self.neg_gamma_g2 != other.neg_gamma_g2 ||
//             cmp_ne_as_g2affine::<E>(self.neg_gamma_g2.clone(), other.neg_gamma_g2.clone()) ||
//             // self.neg_delta_g2 != other.neg_delta_g2 ||
//             cmp_ne_as_g2affine::<E>(self.neg_delta_g2.clone(), other.neg_delta_g2.clone()) ||
//             self.ic.len() != other.ic.len()
//         {
//             return false
//         }
//         for i in 0..self.ic.len() {
//             if self.ic[i] != other.ic[i] {
//                 return false
//             }
//         }
//         true
//     }
// }

// fn cmp_ne_as_g2affine<E: MultiMillerLoop>(p1: E::G2Prepared, p2: E::G2Prepared) -> bool {
//     Into::<E::G2Affine>::into(p1) != Into::<E::G2Affine>::into(p2)
// }

impl<E: MultiMillerLoop> core::fmt::Debug for PreparedVerifyingKey<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "")
    }
}

impl<E: MultiMillerLoop> From<VerifyingKey<E>> for PreparedVerifyingKey<E> {
    fn from(vkey: VerifyingKey<E>) -> Self {
        let gamma = vkey.gamma_g2.neg();
        let delta = vkey.delta_g2.neg();

        PreparedVerifyingKey::<E> {
            alpha_g1_beta_g2: E::pairing(&vkey.alpha_g1, &vkey.beta_g2),
            neg_gamma_g2: gamma.into(),
            neg_delta_g2: delta.into(),
            ic: vkey.ic,
        }
    }
}

/// TODO
#[derive(Clone, Decode, Default, Encode, Eq, TypeInfo)]
pub struct PublicInputs<E: Engine>(pub Vec<E::Fr>);

impl<E: Engine> core::fmt::Debug for PublicInputs<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "")
    }
}

impl<E: Engine> PartialEq for PublicInputs<E> {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        for i in 0..self.0.len() {
            if self.0[i] != other.0[i] {
                return false;
            }
        }
        true
    }
}

impl<E: Engine> alloc::borrow::Borrow<Vec<E::Fr>> for PublicInputs<E> {
    fn borrow(&self) -> &Vec<E::Fr> {
        &self.0
    }
}

impl<E: Engine> alloc::borrow::BorrowMut<Vec<E::Fr>> for PublicInputs<E> {
    fn borrow_mut(&mut self) -> &mut Vec<E::Fr> {
        &mut self.0
    }
}

impl<E> PublicInputs<E>
where
    E: Engine,
    E::Fr: PrimeField<Repr = [u8; 32]>,
{
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::<u8>::new();
        for s in self.0 {
            let repr: [u8; 32] = s.to_repr();
            bytes.append(&mut repr.to_vec());
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<PublicInputs<E>, ()> {
        if bytes.len() % 32 != 0 {
            return Err(());
        }

        let mut inputs = Vec::<E::Fr>::new();
        let mut buffer = [0u8; 32];
        let mut idx = 0;

        while idx <= bytes.len() - 32 {
            idx = copy_next_bytes(&mut buffer, idx, bytes);
            let primefield = <E::Fr as PrimeField>::from_repr(buffer)
                .into_option()
                .ok_or(())?;
            inputs.push(primefield);
        }

        Ok(PublicInputs::<E>(inputs))
    }
}

fn copy_next_bytes(buffer: &mut [u8], mut idx: usize, bytes: &[u8]) -> usize {
    for i in 0..buffer.len() {
        buffer[i] = bytes[idx];
        idx += 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;

    #[test]
    fn proof_into_bytes_and_from_bytes() {
        let proof = Proof::<Bls12> {
            a: G1Affine::generator(),
            b: G2Affine::generator(),
            c: G1Affine::generator(),
        };
        let proof_bytes = proof.clone().into_bytes();
        assert_eq!(proof, Proof::<Bls12>::from_bytes(&proof_bytes).unwrap());
    }

    #[test]
    fn verifyingkey_into_bytes_and_from_bytes() {
        let vkey = VerifyingKey::<Bls12> {
            alpha_g1: G1Affine::generator(),
            beta_g1: G1Affine::generator(),
            beta_g2: G2Affine::generator(),
            gamma_g2: G2Affine::generator(),
            delta_g1: G1Affine::generator(),
            delta_g2: G2Affine::generator(),
            ic: vec![G1Affine::generator(), G1Affine::generator()],
        };
        let vkey_bytes = vkey.clone().into_bytes();
        assert_eq!(
            vkey,
            VerifyingKey::<Bls12>::from_bytes(&vkey_bytes).unwrap()
        );
    }

    #[test]
    fn publicinputs_into_bytes_and_from_bytes() {
        let mut inputs = Vec::<bls12_381::Scalar>::new();
        let mut rng = rand::thread_rng();
        for _ in 0..15 {
            let random_u64: u64 = rng.gen();
            inputs.push(bls12_381::Scalar::from(random_u64));
        }
        let inputs = PublicInputs::<Bls12>(inputs);
        let inputs_bytes = inputs.clone().into_bytes();
        assert_eq!(
            inputs,
            PublicInputs::<Bls12>::from_bytes(&inputs_bytes).unwrap()
        );
    }
}