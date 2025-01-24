use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::BoundedVec;

use crate::{sector::SectorNumber, MAX_SEAL_PROOF_BYTES};

/// Arguments passed into the `prove_commit_sector` extrinsic.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ProveCommitSector {
    pub sector_number: SectorNumber,
    pub proof: BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
}
