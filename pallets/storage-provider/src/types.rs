use scale_info::prelude::string::String;

pub use proofs::{RegisteredPoStProof, RegisteredSealProof};
pub use sector::{SectorOnChainInfo, SectorPreCommitOnChainInfo, SectorSize};
pub use storage_provider::{StorageProviderInfo, StorageProviderState};

mod proofs;
mod sector;
mod storage_provider;

type Cid = String;
