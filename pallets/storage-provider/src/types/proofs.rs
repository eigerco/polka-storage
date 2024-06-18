use crate::types::SectorSize;

use codec::{Decode, Encode};
use scale_info::prelude::vec::Vec;
use scale_info::TypeInfo;

/// Proof of Spacetime type, indicating version and sector size of the proof.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone, Copy)]
pub enum RegisteredPoStProof {
    StackedDRGWindow2KiBV1P1,
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> SectorSize {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 => SectorSize::_2KiB,
        }
    }

    /// Proof size for each PoStProof type
    #[allow(unused)]
    pub fn proof_size(self) -> usize {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 => 192,
        }
    }
    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(self) -> u64 {
        // Resolve to post proof and then compute size from that.
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 => 2,
        }
    }
}

/// Proof of Spacetime data stored on chain.
#[derive(Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
    pub proof_bytes: Vec<u8>,
}

/// Seal proof type which defines the version and sector size.
#[allow(non_camel_case_types)]
#[derive(Debug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
pub enum RegisteredSealProof {
    StackedDRG2KiBV1P1,
}
