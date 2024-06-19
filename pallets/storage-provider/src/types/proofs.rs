use codec::{Decode, Encode};
use frame_support::pallet_prelude::ConstU32;
use frame_support::sp_runtime::BoundedVec;
use scale_info::TypeInfo;

use crate::types::SectorSize;

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
