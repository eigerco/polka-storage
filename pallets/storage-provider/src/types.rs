use primitives::{BlockNumber, DAYS, SLOT_DURATION};
use scale_info::prelude::string::String;

pub use proofs::{RegisteredPoStProof, RegisteredSealProof};
pub use sector::{SectorOnChainInfo, SectorPreCommitOnChainInfo, SectorSize};
pub use storage_provider::{StorageProviderInfo, StorageProviderState};

mod proofs;
mod sector;
mod storage_provider;

type Cid = String;

// Challenge window of 24 hours
pub const WPOST_PROVING_PERIOD: BlockNumber = DAYS;

// Half an hour (=48 per day)
// 30 * 60 = 30 minutes
// SLOT_DURATION is in milliseconds thats why we / 1000
pub const WPOST_CHALLENGE_WINDOW: BlockNumber = 30 * 60 / (SLOT_DURATION as BlockNumber / 1000);
