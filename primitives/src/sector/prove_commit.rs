use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::BoundedVec;

use crate::{sector::SectorNumber, MAX_PROOFS_PER_BLOCK, MAX_SEAL_PROOF_BYTES};

/// Arguments passed into the `prove_commit_sector` extrinsic.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ProveCommitSector {
    pub sector_number: SectorNumber,
    pub proofs:
        BoundedVec<BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>, ConstU32<MAX_PROOFS_PER_BLOCK>>,
}
