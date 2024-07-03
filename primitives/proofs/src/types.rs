use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

pub type DealId = u64;

pub type SectorNumber = u64;

#[allow(non_camel_case_types)]
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
pub enum RegisteredSealProof {
    StackedDRG2KiBV1P1,
}

impl RegisteredSealProof {
    pub fn sector_size(&self) -> SectorSize {
        SectorSize::_2KiB
    }
}

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Copy)]
pub enum SectorSize {
    _2KiB,
}

impl SectorSize {
    /// Returns the size of a sector in bytes
    /// <https://github.com/filecoin-project/ref-fvm/blob/5659196fa94accdf1e7f10e00586a8166c44a60d/shared/src/sector/mod.rs#L40>
    pub fn bytes(&self) -> u64 {
        match self {
            SectorSize::_2KiB => 2 << 10,
        }
    }
}
