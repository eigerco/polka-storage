//! Groth16 ZK-SNARK related implementations.

use core::ops::{AddAssign, Neg};

use codec::{Decode, Encode};
pub use polka_storage_proofs::{Bls12, PrimeField, Proof, Scalar as Fr, VerifyingKey};
use polka_storage_proofs::{Curve, MillerLoopResult, MultiMillerLoop, PrimeCurveAffine};
use scale_info::TypeInfo;

use crate::Vec;

/// The prepared verifying key needed in a Groth16 verification.
///
/// References:
/// - <https://github.com/zkcrypto/bellman/blob/3a1c43b01a89d426842df39b432de979917951e6/groth16/src/lib.rs#L400>
/// - <https://github.com/filecoin-project/bellperson/blob/a594f329b05b6224047903fb51658e8a35a12fbd/src/groth16/verifying_key.rs#L200>
#[derive(Clone, Decode, Default, Encode)]
pub(crate) struct PreparedVerifyingKey<E: MultiMillerLoop> {
    pub alpha_g1_beta_g2: E::Gt,
    pub neg_gamma_g2: E::G2Prepared,
    pub neg_delta_g2: E::G2Prepared,
    pub ic: Vec<E::G1Affine>,
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

/// Method generates the `PreparedVerifyingKey` from the `VerifyingKey`.
///
/// References:
/// - <https://github.com/zkcrypto/bellman/blob/3a1c43b01a89d426842df39b432de979917951e6/groth16/src/verifier.rs#L11>
pub(crate) fn prepare_verifying_key<E: MultiMillerLoop>(
    vkey: VerifyingKey<E>,
) -> PreparedVerifyingKey<E> {
    PreparedVerifyingKey::<E>::from(vkey)
}

/// Verifies a single Groth16 ZK-SNARK proof by using the given prepared verifying key, the proof
/// and the public inputs. Currently, this code is closer aligned to `bellman`'s implementation
/// than to `bellperson`'s implementation due to the complexity of parallel computing.
///
/// References:
/// - <https://github.com/zkcrypto/bellman/blob/3a1c43b01a89d426842df39b432de979917951e6/groth16/src/verifier.rs#L23>
/// - <https://github.com/filecoin-project/bellperson/blob/a594f329b05b6224047903fb51658e8a35a12fbd/src/groth16/verifier.rs#L38>
pub(crate) fn verify_proof<'a, E: MultiMillerLoop>(
    pvk: &'a PreparedVerifyingKey<E>,
    proof: &Proof<E>,
    public_inputs: &[E::Fr],
) -> Result<(), VerificationError> {
    if (public_inputs.len() + 1) != pvk.ic.len() {
        return Err(VerificationError::InvalidVerifyingKey);
    }

    let mut acc = pvk.ic[0].to_curve();

    for (i, b) in public_inputs.iter().zip(pvk.ic.iter().skip(1)) {
        AddAssign::<&E::G1>::add_assign(&mut acc, &(*b * i));
    }

    // The original verification equation is:
    // A * B = alpha * beta + inputs * gamma + C * delta
    // ... however, we rearrange it so that it is:
    // A * B - inputs * gamma - C * delta = alpha * beta
    // or equivalently:
    // A * B + inputs * (-gamma) + C * (-delta) = alpha * beta
    // which allows us to do a single final exponentiation.

    if pvk.alpha_g1_beta_g2
        == E::multi_miller_loop(&[
            (&proof.a, &proof.b.into()),
            (&acc.to_affine(), &pvk.neg_gamma_g2),
            (&proof.c, &pvk.neg_delta_g2),
        ])
        .final_exponentiation()
    {
        Ok(())
    } else {
        Err(VerificationError::InvalidProof)
    }
}

/// Possible error types in a failed Groth16 ZK-SNARK verification.
#[derive(Clone, Debug, Decode, Eq, Encode, PartialEq, TypeInfo)]
pub enum VerificationError {
    /// Returned when the given proof was invalid in a verification.
    InvalidProof,
    /// Returned when the given verifying key was invalid.
    InvalidVerifyingKey,
}
