mod number;
mod pre_commit;
mod prove_commit;
mod size;

// NOTE(@jmg-duarte,16/01/2025): unsure if the visitor should be exposed
pub use number::{SectorNumber, SectorNumberError};
pub use pre_commit::SectorPreCommitInfo;
pub use prove_commit::ProveCommitSector;
pub use size::SectorSize;

// `test` is only useful locally
#[cfg(any(test, feature = "builder"))]
pub mod builder {
    pub use crate::sector::pre_commit::builder::SectorPreCommitInfoBuilder;
}
