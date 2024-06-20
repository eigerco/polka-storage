use scale_info::prelude::string::String;

mod proofs;
mod sector;
mod storage_provider;

pub use proofs::{
    assign_proving_period_offset, current_deadline_index, current_proving_period_start,
    RegisteredPoStProof, RegisteredSealProof,
};
pub use sector::{SectorOnChainInfo, SectorPreCommitOnChainInfo, SectorSize};
pub use storage_provider::{StorageProviderInfo, StorageProviderState};

type Cid = String;
