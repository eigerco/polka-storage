use codec::{Decode, Encode};
use frame_support::{pallet_prelude::ConstU32, sp_runtime::BoundedVec};
use primitives_proofs::RegisteredPoStProof;
use scale_info::TypeInfo;
use sp_arithmetic::traits::BaseArithmetic;
use sp_core::blake2_64;

use crate::partition::PartitionNumber;

/// Proof of Spacetime data stored on chain.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    /// The proof type, currently only one type is supported.
    pub post_proof: RegisteredPoStProof,
    /// The proof submission, to be checked in the storage provider pallet.
    pub proof_bytes: BoundedVec<u8, ConstU32<256>>, // Arbitrary length
}

/// Parameter type for `submit_windowed_post` extrinsic.
// Filecoin has the partitions and the proofs in an array but then checks that there is only one element in the array.
// Why even use the array?
// ref: <https://github.com/filecoin-project/builtin-actors/blob/8d957d2901c0f2044417c268f0511324f591cb92/actors/miner/src/lib.rs#L510>
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct SubmitWindowedPoStParams<BlockNumber> {
    /// The deadline index which the submission targets.
    pub deadline: u64,
    /// The partition being proven.
    pub partition: PartitionNumber,
    /// Proof submission
    pub proof: PoStProof,
    /// The block at which these proofs is being committed.
    pub chain_commit_block: BlockNumber,
}

/// Error type for proof operations.
#[derive(Debug)]
pub enum ProofError {
    Conversion,
}

/// Assigns proving period offset randomly in the range [0, WPOST_PROVING_PERIOD)
/// by hashing the address and current block number.
///
/// Filecoin implementation reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4886>
pub(crate) fn assign_proving_period_offset<AccountId, BlockNumber>(
    addr: &AccountId,
    current_block: BlockNumber,
    wpost_proving_period: BlockNumber,
) -> Result<BlockNumber, ProofError>
where
    AccountId: Encode,
    BlockNumber: BaseArithmetic + Encode + TryFrom<u64>,
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

/// Computes the block at which a proving period should start such that it is greater than the current block, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A storage provider is exempt from Window PoSt until the first full proving period starts.
/// Filecoin implementation reference: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4907
pub(crate) fn current_proving_period_start<BlockNumber>(
    current_block: BlockNumber,
    offset: BlockNumber,
    proving_period: BlockNumber, // should be the max proving period
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    // Use this value to calculate the proving period start, modulo the proving period so we cannot go over the max proving period
    // the value represents how far into a proving period we are.
    let how_far_into_proving_period = current_block.clone() % proving_period.clone();
    let period_progress = if how_far_into_proving_period >= offset {
        how_far_into_proving_period - offset
    } else {
        proving_period - (offset - how_far_into_proving_period)
    };
    if current_block < period_progress {
        period_progress
    } else {
        current_block - period_progress
    }
}

/// Filecoin implementation reference: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4923
pub(crate) fn current_deadline_index<BlockNumber>(
    current_block: BlockNumber,
    period_start: BlockNumber,
    challenge_window: BlockNumber,
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    match current_block.checked_sub(&period_start) {
        Some(block) => block / challenge_window,
        None => period_start / challenge_window,
    }
}
