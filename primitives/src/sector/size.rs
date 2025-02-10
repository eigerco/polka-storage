//! Sector size primitive.

use codec::{Decode, Encode};
use scale_decode::DecodeAsType;
use scale_encode::EncodeAsType;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(
    Encode, Decode, DecodeAsType, EncodeAsType, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Copy,
)]
#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[codec(crate = ::codec)]
#[decode_as_type(crate_path = "::scale_decode")]
#[encode_as_type(crate_path = "::scale_encode")]
pub enum SectorSize {
    _2KiB,
    _8MiB,
    _512MiB,
    _1GiB,
    _32GiB,
    _64GiB,
}

impl SectorSize {
    /// Returns the size of a sector in bytes
    /// <https://github.com/filecoin-project/ref-fvm/blob/5659196fa94accdf1e7f10e00586a8166c44a60d/shared/src/sector/mod.rs#L40>
    pub fn bytes(&self) -> u64 {
        match self {
            SectorSize::_2KiB => 2 << 10,
            SectorSize::_8MiB => 8 << 20,
            SectorSize::_512MiB => 512 << 20,
            SectorSize::_1GiB => 1 << 30,
            SectorSize::_32GiB => 32 << 30,
            SectorSize::_64GiB => 2 * (32 << 30),
        }
    }
}

impl core::fmt::Display for SectorSize {
    fn fmt(
        &self,
        f: &mut scale_info::prelude::fmt::Formatter<'_>,
    ) -> scale_info::prelude::fmt::Result {
        match self {
            SectorSize::_2KiB => write!(f, "2KiB"),
            SectorSize::_8MiB => write!(f, "8MiB"),
            SectorSize::_512MiB => write!(f, "512MiB"),
            SectorSize::_1GiB => write!(f, "1GiB"),
            SectorSize::_32GiB => write!(f, "32GiB"),
            SectorSize::_64GiB => write!(f, "64GiB"),
        }
    }
}
