use codec::Encode;
use primitives::BlockNumber;
use sp_core::blake2_64;

use crate::types::{WPOST_CHALLENGE_WINDOW, WPOST_PROVING_PERIOD};

/// Assigns proving period offset randomly in the range [0, WPOST_PROVING_PERIOD)
/// by hashing the address and current block number.
///
/// Filecoin implementation reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4886>
pub fn assign_proving_period_offset<AccountId>(
    addr: &AccountId,
    current_block: BlockNumber,
) -> BlockNumber
where
    AccountId: Encode,
{
    let mut addr = addr.encode();
    let mut block_num = current_block.to_be_bytes().to_vec();

    addr.append(&mut block_num);

    let digest = blake2_64(&addr);

    let mut offset = u64::from_be_bytes(digest) as u32;

    offset %= WPOST_PROVING_PERIOD;

    offset
}

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Window PoSt until the first full proving period starts.
//
/// Filecoin implementation reference: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4907
pub fn current_proving_period_start(
    current_block: BlockNumber,
    offset: BlockNumber,
) -> BlockNumber {
    let curr_modulus = current_block % WPOST_PROVING_PERIOD;

    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        WPOST_PROVING_PERIOD - (offset - curr_modulus)
    };

    current_block - period_progress
}

/// Filecoin implementation reference: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L4923
pub fn current_deadline_index(
    current_block: BlockNumber,
    period_start: BlockNumber,
) -> BlockNumber {
    (current_block - period_start) / WPOST_CHALLENGE_WINDOW
}
