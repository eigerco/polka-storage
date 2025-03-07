//! Groth16 ZK-SNARK related implementations.

use core::ops::{AddAssign, Neg, Mul, MulAssign};

use bls12_381::{multi_miller_loop, G2Prepared};
use codec::{Decode, Encode};
use pairing::{Engine, MillerLoopResult, group::Group};
use ff::Field;
pub use polka_storage_proofs::{Bls12, PrimeField, Proof, Scalar as Fr, VerifyingKey};
use polka_storage_proofs::{Curve, MultiMillerLoop, PrimeCurveAffine};
use primitives::randomness::{draw_randomness, DomainSeparationTag};
use rand::SeedableRng;
use scale_info::TypeInfo;

use crate::{fr32::bytes_into_fr_repr_safe, Vec};

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

pub(crate) fn le_bytes_to_u64s(le_bytes: &[u8]) -> Vec<u64> {
    assert_eq!(
        le_bytes.len() % 8,
        0,
        "length must be divisible by u64 byte length (8-bytes)"
    );
    le_bytes
        .chunks(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

pub fn verify_proofs_batch<E>(
    pvk: &PreparedVerifyingKey<E>,
    // rng: &mut R,
    proofs: &[Proof<E>],
    public_inputs: &[Vec<E::Fr>],
) -> Result<bool, VerificationError>
where
    E: MultiMillerLoop,
    <E::Fr as PrimeField>::Repr: Sync + Copy,
    // R: rand::RngCore,
{
    debug_assert_eq!(proofs.len(), public_inputs.len());

    for pub_input in public_inputs {
        if (pub_input.len() + 1) != pvk.ic.len() {
            return Err(VerificationError::InvalidInput);
        }
    }

    let num_inputs = public_inputs[0].len();
    let num_proofs = proofs.len();

    log::debug!("num proofs? woot: {}", num_proofs);
    if num_proofs < 2 {
        return verify_proof(pvk, &proofs[0], &public_inputs[0]);
    }


    log::debug!("ok going down");
    let proof_num = proofs.len();

    // Choose random coefficients for combining the proofs
    let mut rand_z_repr: Vec<_> = Vec::with_capacity(proof_num);
    let mut rand_z: Vec<_> = Vec::with_capacity(proof_num);
    let mut accum_y = E::Fr::ZERO;

    use rand::Rng;
    use rand_xorshift::XorShiftRng;
    let rng = &mut XorShiftRng::from_seed([
        0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0xd, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
    ]);

    log::debug!("generating random numbers");
    for _ in 0..proof_num {
        let t: u128 = rng.gen();

        let mut repr = E::Fr::ZERO.to_repr();
        let mut repr_u64s = le_bytes_to_u64s(repr.as_ref());
        assert!(repr_u64s.len() > 1);

        repr_u64s[0] = (t & (-1i64 as u128) >> 64) as u64;
        repr_u64s[1] = (t >> 64) as u64;

        for (i, limb) in repr_u64s.iter().enumerate() {
            let start = i * 8;
            let stop = start + 8;
            repr.as_mut()[start..stop].copy_from_slice(&limb.to_le_bytes());
        }

        let fr = E::Fr::from_repr(repr).unwrap();
        let repr = fr.to_repr();

        accum_y.add_assign(&fr);
        rand_z_repr.push(repr);
        rand_z.push(fr);
    }
    log::debug!("generated random numbers");

    log::debug!("acc_g start");
    // Calculate Accum_Gamma sequentially
    let mut acc_g = E::G1::identity();
    for i in 0..(num_inputs + 1) {
        let scalar = if i == 0 {
            accum_y
        } else {
            let idx = i - 1;
            let mut cur_sum = rand_z[0];
            cur_sum.mul_assign(&public_inputs[0][idx]);
            
            for (pi_mont, mut rand_mont) in 
                public_inputs.iter().zip(rand_z.iter().copied()).skip(1)
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
    log::debug!("ml_g done");

    // Calculate Accum_Delta sequentially
    let mut acc_d = E::G1::identity();
    for (proof, rand) in proofs.iter().zip(rand_z.iter()) {
        let term = proof.c.mul(*rand);
        acc_d.add_assign(&term);
    }
    let ml_d = E::multi_miller_loop(&[(&acc_d.to_affine(), &pvk.delta_g2)]);
    log::debug!("ml_d done");

    // Calculate Accum_AB sequentially 
    // OLD
    let mut acc_ab = <E as MultiMillerLoop>::Result::default();
    for (proof, rand) in proofs.iter().zip(rand_z.iter()) {
        let mul_a = proof.a.mul(*rand);
        let cur_neg_b = -proof.b.to_curve();
        let term = E::multi_miller_loop(&[(&mul_a.to_affine(), &cur_neg_b.to_affine().into())]);
        acc_ab += term;
    }
    log::debug!("acc_ab done");

    // v2
    // let mut pairs = Vec::with_capacity(num_proofs + 2);

    // for (proof, rand) in proofs.iter().zip(rand_z.iter()) {
    //     let mul_a = proof.a.mul(*rand).to_affine();
    //     let neg_b: E::G2Prepared = (-proof.b).into();
    //     pairs.push((mul_a, neg_b));
    // }
    // let acc_d_aff = acc_d.to_affine();
    // let acc_g_aff = acc_g.to_affine();
    // pairs.push((acc_d_aff, pvk.delta_g2.into()));
    // pairs.push((acc_g_aff, pvk.gamma_g2.into()));

    /* // Step 1: Store owned values in vectors
    let mut mul_a_vec: Vec<E::G1Affine> = Vec::with_capacity(num_proofs);
    let mut neg_b_vec: Vec<E::G2Prepared> = Vec::with_capacity(num_proofs);

    for (proof, rand) in proofs.iter().zip(rand_z.iter()) {
        let mul_a = proof.a.mul(*rand).to_affine(); // Compute G1Affine
        let neg_b: E::G2Prepared = (-proof.b).into(); // Compute G2Prepared
        mul_a_vec.push(mul_a); // Store owned value
        neg_b_vec.push(neg_b); // Store owned value
    }

    // Step 2: Create pairs with references to the stored values
    let mut pairs: Vec<(&E::G1Affine, &E::G2Prepared)> = Vec::with_capacity(num_proofs);
    for (mul_a, neg_b) in mul_a_vec.iter().zip(neg_b_vec.iter()) {
        pairs.push((mul_a, neg_b)); // References to owned data
    }

    let mut ml_all = E::multi_miller_loop(&pairs); */
    let mut ml_all = acc_ab;
    ml_all += ml_d;
    ml_all += ml_g;

    // Calculate Y^-Accum_Y
    let accum_y_neg = -accum_y;
    let y = pvk.alpha_g1_beta_g2 * accum_y_neg;

    let actual = ml_all.final_exponentiation();
    Ok(actual == y)
}