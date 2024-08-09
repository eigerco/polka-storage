use bls12_381::{G1Affine, G2Affine};
use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::{ConstU32, RuntimeDebug},
    sp_runtime::BoundedVec,
};
use pairing::group::{prime::PrimeCurveAffine, Curve};
use pairing::{Engine, MillerLoopResult, MultiMillerLoop};
use primitives_proofs::RegisteredPoStProof;
use scale_info::TypeInfo;
use sp_core::blake2_64;
use sp_std::ops::{AddAssign, Neg};

use crate::{
    pallet::Error,
    partition::{PartitionNumber, MAX_PARTITIONS_PER_DEADLINE},
};

/// Proof of Spacetime data stored on chain.
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    /// The proof type, currently only one type is supported.
    pub post_proof: RegisteredPoStProof,
    /// The proof submission, to be checked in the storage provider pallet.
    pub proof_bytes: BoundedVec<u8, ConstU32<384>>,
    /// The verifying key for the given proof.
    pub vkey_bytes: BoundedVec<u8, ConstU32<1056>>,
}

/// Parameter type for `submit_windowed_post` extrinsic.
// In filecoind the proof is an array of proofs, one per distinct registered proof type present in the sectors being proven.
// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/types.rs#L114-L115>
// We differ here from Filecoin and do not support registration of different proof types.
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct SubmitWindowedPoStParams {
    /// The deadline index which the submission targets.
    pub deadline: u64,
    /// The partition being proven.
    pub partitions: BoundedVec<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
    /// The proof submission.
    pub proof: PoStProof,
}

/// Error type for proof operations.
#[derive(RuntimeDebug)]
pub enum ProofError {
    /// Conversion error, e.g. from streamed bytes to struct definition.
    Conversion,
    /// The given Windowed PoSt proof itself is not valid.
    InvalidProof,
    /// The given verification key is not valid.
    InvalidVerifyingKey,
}

impl<T> From<ProofError> for Error<T> {
    fn from(value: ProofError) -> Self {
        match value {
            ProofError::Conversion => Error::<T>::ConversionError,
            ProofError::InvalidProof => Error::<T>::PoStProofInvalid, // TODO
            ProofError::InvalidVerifyingKey => Error::<T>::InvalidVerifyingKey,
        }
    }
}

/// Assigns proving period offset randomly in the range [0, WPOST_PROVING_PERIOD)
/// by hashing the address and current block number.
///
/// Reference:
/// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4886>
pub(crate) fn assign_proving_period_offset<AccountId, BlockNumber>(
    addr: &AccountId,
    current_block: BlockNumber,
    wpost_proving_period: BlockNumber,
) -> Result<BlockNumber, ProofError>
where
    AccountId: Encode,
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    // Encode address and current block number
    let mut addr = addr.encode();
    let mut block_num = current_block.encode();
    // Concatenate the encoded block number to the encoded address.
    addr.append(&mut block_num);
    // Hash the address and current block number for a pseudo-random offset.
    let digest = blake2_64(&addr);
    // Create a pseudo-random offset from the bytes of the hash of the address and current block number.
    let offset = u64::from_be_bytes(digest);
    // Convert into block number
    let mut offset =
        TryInto::<BlockNumber>::try_into(offset).map_err(|_| ProofError::Conversion)?;
    // Mod with the proving period so it is within the valid range of [0, WPOST_PROVING_PERIOD)
    offset %= wpost_proving_period;
    Ok(offset)
}

// TODO remove personal notes
// Output datatypes of methods
// - generate_window_post: SnarkProof ( Vec<groth16::Proof<Bls12>> )
// - generate_winning_post: SnarkProof ( Vec<groth16::Proof<Bls12>> )
// - ...

/// This constant specifies the number of bytes of a serialized `Proof`.
pub const PROOF_BYTES: usize = 384;

/// For more information on this definition check out the `bellperson`'s definition.
#[derive(Clone, Decode, Default, Encode, Eq, RuntimeDebug)]
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
    /// Turns the proof into `Vec<u8>`.
    #[allow(dead_code)]
    pub fn into_bytes(self) -> [u8; PROOF_BYTES] {
        let mut bytes = [0u8; PROOF_BYTES];
        let mut idx = 0;

        for b in self.a.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.b.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.c.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }

        bytes
    }

    /// Tries to create a `Proof` from given bytes.
    pub fn from_bytes(bytes: [u8; PROOF_BYTES]) -> Result<Proof<E>, ProofError> {
        let mut g1_chunk = [0u8; 96];
        let mut g2_chunk = [0u8; 192];
        let mut idx = 0;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let a = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..192 {
            g2_chunk[i] = bytes[idx];
            idx += 1;
        }
        let b = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let c = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        Ok(Proof::<E> { a, b, c })
    }
}

/// This constant specifies the number of bytes of a serialized `Proof`.
pub const VERIFYINGKEY_BYTES: usize = 1056;

/// For more information on this definition check out the `bellperson`'s definition.
#[derive(Clone, Decode, Default, Encode, Eq, RuntimeDebug)]
pub struct VerifyingKey<E: Engine> {
    pub alpha_g1: E::G1Affine,
    pub beta_g1: E::G1Affine,
    pub beta_g2: E::G2Affine,
    pub gamma_g2: E::G2Affine,
    pub delta_g1: E::G1Affine,
    pub delta_g2: E::G2Affine,
    pub ic: [E::G1Affine; 2], // number of inputs
}

impl<E: Engine> PartialEq for VerifyingKey<E> {
    fn eq(&self, other: &Self) -> bool {
        self.alpha_g1 == other.alpha_g1 && self.beta_g1 == other.beta_g1
            && self.beta_g2 == other.beta_g2 && self.gamma_g2 == other.gamma_g2
            && self.delta_g1 == other.delta_g1 && self.delta_g2 == other.delta_g2
            && self.ic == other.ic
    }
}

impl<E> VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    #[allow(dead_code)]
    pub fn into_bytes(self) -> [u8; VERIFYINGKEY_BYTES] {
        let mut bytes = [0u8; VERIFYINGKEY_BYTES];
        let mut idx = 0;

        for b in self.alpha_g1.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.beta_g1.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.beta_g2.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.gamma_g2.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.delta_g1.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.delta_g2.to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.ic[0].to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }
        for b in self.ic[1].to_uncompressed() {
            bytes[idx] = b;
            idx += 1;
        }

        bytes
    }

    pub fn from_bytes(bytes: [u8; VERIFYINGKEY_BYTES]) -> Result<VerifyingKey<E>, ProofError> {
        let mut g1_chunk = [0u8; 96];
        let mut g2_chunk = [0u8; 192];
        let mut idx = 0;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let alpha_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let beta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..192 {
            g2_chunk[i] = bytes[idx];
            idx += 1;
        }
        let beta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..192 {
            g2_chunk[i] = bytes[idx];
            idx += 1;
        }
        let gamma_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let delta_g1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..192 {
            g2_chunk[i] = bytes[idx];
            idx += 1;
        }
        let delta_g2 = G2Affine::from_uncompressed(&g2_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let ic1 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        for i in 0..96 {
            g1_chunk[i] = bytes[idx];
            idx += 1;
        }
        let ic2 = G1Affine::from_uncompressed(&g1_chunk)
            .into_option()
            .ok_or(ProofError::Conversion)?;

        Ok(VerifyingKey::<E> {
            alpha_g1,
            beta_g1,
            beta_g2,
            gamma_g2,
            delta_g1,
            delta_g2,
            ic: [ic1, ic2],
        })
    }
}

/// For more information on this definition check out the `bellperson`'s definition.
#[derive(Clone, Encode, Decode, Default, PartialEq, Eq)]
pub struct PreparedVerifyingKey<E: MultiMillerLoop> {
    alpha_g1_beta_g2: E::Gt,
    neg_gamma_g2: E::G2Prepared,
    neg_delta_g2: E::G2Prepared,
    ic: [E::G1Affine; 2],
}

pub fn prepare_verifying_key<E: MultiMillerLoop>(vk: &VerifyingKey<E>) -> PreparedVerifyingKey<E> {
    let gamma = vk.gamma_g2.neg();
    let delta = vk.delta_g2.neg();

    PreparedVerifyingKey {
        alpha_g1_beta_g2: E::pairing(&vk.alpha_g1, &vk.beta_g2),
        neg_gamma_g2: gamma.into(),
        neg_delta_g2: delta.into(),
        ic: vk.ic.clone(),
    }
}

pub fn verify_proof<'a, E: MultiMillerLoop>(
    pvk: &'a PreparedVerifyingKey<E>,
    proof: &Proof<E>,
    public_inputs: &[E::Fr],
) -> Result<(), ProofError> {
    if (public_inputs.len() + 1) != pvk.ic.len() {
        return Err(ProofError::InvalidVerifyingKey);
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
        Err(ProofError::InvalidProof)
    }
}

#[cfg(test)]
mod tests {
    use bls12_381::Bls12;
    use super::*;

    #[test]
    fn proof_into_bytes_and_back() {
        let proof = Proof::<Bls12>{
            a: G1Affine::generator(),
            b: G2Affine::generator(),
            c: G1Affine::generator(),
        };
        let proof_bytes = proof.clone().into_bytes();
        assert_eq!(proof, Proof::<Bls12>::from_bytes(proof_bytes).unwrap());
    }

    #[test]
    fn verifyingkey_into_bytes_and_back() {
        let vkey = VerifyingKey::<Bls12>{
            alpha_g1: G1Affine::generator(),
            beta_g1: G1Affine::generator(),
            beta_g2: G2Affine::generator(),
            gamma_g2: G2Affine::generator(),
            delta_g1: G1Affine::generator(),
            delta_g2: G2Affine::generator(),
            ic: [G1Affine::generator(), G1Affine::generator()],
        };
        let vkey_bytes = vkey.clone().into_bytes();
        assert_eq!(vkey, VerifyingKey::<Bls12>::from_bytes(vkey_bytes).unwrap());
    }

    #[ignore = "to be implemented"]
    #[test]
    fn verify_proof_ok_on_valid_proof() {
    }

    #[ignore = "to be implemented"]
    #[test]
    fn verify_proof_err_on_invalid_proof() {
    }

    #[ignore = "to be implemented"]
    #[test]
    fn verify_proof_err_on_invalid_verifyingkey() {
    }
}
