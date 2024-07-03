use codec::{Decode, Encode};
use frame_support::{pallet_prelude::ConstU32, sp_runtime::BoundedVec};
use scale_info::TypeInfo;
use sp_arithmetic::traits::BaseArithmetic;
use sp_core::blake2_64;

use crate::sector::SectorSize;

/// Proof of Spacetime type, indicating version and sector size of the proof.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone, Copy)]
pub enum RegisteredPoStProof {
    StackedDRGWindow2KiBV1P1,
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> SectorSize {
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => SectorSize::_2KiB,
        }
    }

    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(self) -> u64 {
        // Resolve to post proof and then compute size from that.
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => 2,
        }
    }
}

/// Proof of Spacetime data stored on chain.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
    pub proof_bytes: BoundedVec<u8, ConstU32<256>>, // Arbitrary length
}

/// Seal proof type which defines the version and sector size.
#[allow(non_camel_case_types)]
#[derive(Debug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
pub enum RegisteredSealProof {
    StackedDRG2KiBV1P1,
}

impl RegisteredSealProof {
    /// Produces the windowed PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_window_post_proof(&self) -> RegisteredPoStProof {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => {
                RegisteredPoStProof::StackedDRGWindow2KiBV1P1
            }
        }
    }
}

#[derive(Debug)]
pub enum ProofError {
    Conversion,
}

/// Assigns proving period offset randomly in the range [0, WPOST_PROVING_PERIOD)
/// by hashing the address and current block number.
///
/// Filecoin implementation reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4886>
pub fn assign_proving_period_offset<AccountId, BlockNumber>(
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

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Window PoSt until the first full proving period starts.
//
/// Filecoin implementation reference: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4907
pub fn current_proving_period_start<BlockNumber>(
    current_block: BlockNumber,
    offset: BlockNumber,
    proving_period: BlockNumber,
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    let curr_modulus = current_block.clone() % proving_period.clone();
    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        proving_period - (offset - curr_modulus)
    };
    if current_block < period_progress {
        period_progress
    } else {
        current_block - period_progress
    }
}

/// Filecoin implementation reference: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4923
pub fn current_deadline_index<BlockNumber>(
    current_block: BlockNumber,
    period_start: BlockNumber,
    challenge_window: BlockNumber,
) -> BlockNumber
where
    BlockNumber: BaseArithmetic,
{
    if current_block < period_start {
        period_start / challenge_window
    } else {
        (current_block - period_start) / challenge_window
    }
}
