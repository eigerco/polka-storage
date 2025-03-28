//! Groth16 ZK-SNARK related implementations.

use core::ops::{AddAssign, Mul, MulAssign, Neg};

use codec::{Decode, Encode};
use ff::Field;
use pairing::{group::Group, MillerLoopResult};
pub use polka_storage_proofs::{Bls12, PrimeField, Proof, Scalar as Fr, VerifyingKey};
use polka_storage_proofs::{Curve, MultiMillerLoop, PrimeCurveAffine};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;
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
    pub gamma_g2: E::G2Prepared,
    pub delta_g2: E::G2Prepared,
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
            gamma_g2: vkey.gamma_g2.into(),
            delta_g2: vkey.delta_g2.into(),
            neg_gamma_g2: gamma.into(),
            neg_delta_g2: delta.into(),
            ic: vkey.ic,
        }
    }
}

/// Generates the `PreparedVerifyingKey` from the `VerifyingKey`.
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
pub fn verify_proof<E: MultiMillerLoop>(
    pvk: &PreparedVerifyingKey<E>,
    proof: &Proof<E>,
    public_inputs: &[E::Fr],
) -> Result<bool, VerificationError> {
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
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Possible error types in a failed Groth16 ZK-SNARK verification.
#[derive(Clone, Debug, Decode, Eq, Encode, PartialEq, TypeInfo)]
pub enum VerificationError {
    /// Returned when the given proof was invalid in a verification.
    InvalidProof,
    /// Returned when the given verifying key was invalid.
    InvalidVerifyingKey,
    InvalidInput,
}

fn seeded_rng(seed_bytes: &[u8; 32]) -> XorShiftRng {
    let mut xored = [0u8; 16];
    for i in 0..16 {
        xored[i] = seed_bytes[i] ^ seed_bytes[i + 16];
    }
    XorShiftRng::from_seed(xored)
}

/// Verifies multiple proofs using randomized batch verification.
/// Performance benefit of this approach arises from computing two of the three Miller loops, and the final
/// exponentation, per batch instead of per proof.
///
/// IMPORTANT! This code was not properly audited.
/// There is room for improvement, as efficient multiscalar multiplication wasn't implemented here.
/// Reference:
/// * https://zips.z.cash/protocol/protocol.pdf (Appendix B.2)
/// * https://github.com/filecoin-project/bellperson/blob/95fd3fc10e740547b53ce8e86a04c49509af6a41/src/groth16/verifier.rs#L109
pub fn verify_proofs_batch<E>(
    pvk: &PreparedVerifyingKey<E>,
    seed: &[u8; 32],
    proofs: &[Proof<E>],
    public_inputs: &[Vec<E::Fr>],
) -> Result<bool, VerificationError>
where
    E: MultiMillerLoop,
    <E::Fr as PrimeField>::Repr: Sync + Copy,
{
    debug_assert_eq!(proofs.len(), public_inputs.len());

    for pub_input in public_inputs {
        if (pub_input.len() + 1) != pvk.ic.len() {
            return Err(VerificationError::InvalidVerifyingKey);
        }
    }

    let num_inputs = public_inputs[0].len();
    let num_proofs = proofs.len();

    if num_proofs < 2 {
        return verify_proof(pvk, &proofs[0], &public_inputs[0]);
    }

    let proof_num = proofs.len();

    // Choose random coefficients for combining the proofs
    let mut rand_z_repr: Vec<_> = Vec::with_capacity(proof_num);
    let mut rand_z: Vec<_> = Vec::with_capacity(proof_num);
    let mut accum_y = E::Fr::ZERO;

    let mut rng = seeded_rng(seed);
    for _ in 0..proof_num {
        let fr = E::Fr::random(&mut rng);
        let repr = fr.to_repr();

        accum_y.add_assign(&fr);
        rand_z_repr.push(repr);
        rand_z.push(fr);
    }

    let mut acc_g = E::G1::identity();
    for i in 0..(num_inputs + 1) {
        let scalar = if i == 0 {
            accum_y
        } else {
            let idx = i - 1;
            let mut cur_sum = rand_z[0];
            cur_sum.mul_assign(&public_inputs[0][idx]);

            for (pi_mont, mut rand_mont) in public_inputs.iter().zip(rand_z.iter().copied()).skip(1)
            {
                let pi_mont = &pi_mont[idx];
                rand_mont.mul_assign(pi_mont);
                cur_sum.add_assign(&rand_mont);
            }
            cur_sum
        };

        let term = pvk.ic[i].mul(scalar);
        acc_g.add_assign(&term);
    }
    let ml_g = E::multi_miller_loop(&[(&acc_g.to_affine(), &pvk.gamma_g2)]);

    let mut acc_d = E::G1::identity();
    for (proof, rand) in proofs.iter().zip(rand_z.iter()) {
        let term = proof.c.mul(*rand);
        acc_d.add_assign(&term);
    }
    let ml_d = E::multi_miller_loop(&[(&acc_d.to_affine(), &pvk.delta_g2)]);

    let mut acc_ab = <E as MultiMillerLoop>::Result::default();
    for (proof, rand) in proofs.iter().zip(rand_z.iter()) {
        let mul_a = proof.a.mul(*rand);
        let cur_neg_b = -proof.b.to_curve();
        let term = E::multi_miller_loop(&[(&mul_a.to_affine(), &cur_neg_b.to_affine().into())]);
        acc_ab += term;
    }

    let mut ml_all = acc_ab;
    ml_all += ml_d;
    ml_all += ml_g;

    // Calculate Y^-Accum_Y
    let accum_y_neg = -accum_y;
    let y = pvk.alpha_g1_beta_g2 * accum_y_neg;

    let actual = ml_all.final_exponentiation();
    Ok(actual == y)
}
