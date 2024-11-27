use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::{ConstU32, RuntimeDebug},
    sp_runtime::BoundedVec,
};
use primitives::proofs::{RegisteredPoStProof, MAX_POST_PROOF_BYTES};
use scale_info::TypeInfo;
use sp_core::blake2_64;

use crate::partition::{PartitionNumber, MAX_PARTITIONS_PER_DEADLINE};

/// Proof of Spacetime data stored on chain.
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    /// The proof type, currently only one type is supported.
    pub post_proof: RegisteredPoStProof,
    /// The proof submission, to be checked by [`ProofVerification::verify_post`], usually [`pallet_proofs`].
    pub proof_bytes: BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
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
    Conversion,
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
