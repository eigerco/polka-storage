use codec::{Decode, Encode};
use scale_decode::DecodeAsType;
use scale_encode::EncodeAsType;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

pub type DealId = u64;

// TODO(#129,@cernicc,11/07/2024): Refactor to new type. Sector number should
// always be between 0 and SECTORS_MAX (32 << 20).
pub type SectorNumber = u64;

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

#[allow(non_camel_case_types)]
#[derive(
    Debug, Decode, Encode, DecodeAsType, EncodeAsType, TypeInfo, Eq, PartialEq, Clone, Copy,
)]
#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[codec(crate = ::codec)]
#[decode_as_type(crate_path = "::scale_decode")]
#[encode_as_type(crate_path = "::scale_encode")]
/// References:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L18>
pub enum RegisteredSealProof {
    #[cfg_attr(feature = "serde", serde(alias = "2KiB"))]
    StackedDRG2KiBV1P1,
}

impl RegisteredSealProof {
    pub fn sector_size(&self) -> SectorSize {
        SectorSize::_2KiB
    }

    /// Produces the windowed PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_window_post_proof(&self) -> RegisteredPoStProof {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => {
                RegisteredPoStProof::StackedDRGWindow2KiBV1P1
            }
        }
    }
}

/// Proof of Spacetime type, indicating version and sector size of the proof.
#[derive(
    Debug, Decode, Encode, DecodeAsType, EncodeAsType, TypeInfo, PartialEq, Eq, Clone, Copy,
)]
#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[codec(crate = ::codec)]
#[decode_as_type(crate_path = "::scale_decode")]
#[encode_as_type(crate_path = "::scale_encode")]
pub enum RegisteredPoStProof {
    #[cfg_attr(feature = "serde", serde(alias = "2KiB"))]
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

// serde_json requires std, hence, to test the serialization, we need:
// * test (duh!)
// * serde — (duh!)
// * std — because of serde_json
#[cfg(all(test, feature = "std", feature = "serde"))]
mod serde_tests {
    use crate::{RegisteredPoStProof, RegisteredSealProof};

    #[test]
    fn ensure_serde_for_registered_seal_proof() {
        assert_eq!(
            serde_json::from_str::<RegisteredSealProof>(r#""2KiB""#).unwrap(),
            RegisteredSealProof::StackedDRG2KiBV1P1
        );
        assert_eq!(
            serde_json::from_str::<RegisteredSealProof>(r#""StackedDRG2KiBV1P1""#).unwrap(),
            RegisteredSealProof::StackedDRG2KiBV1P1
        );
    }

    #[test]
    fn ensure_serde_for_registered_post_proof() {
        assert_eq!(
            serde_json::from_str::<RegisteredPoStProof>(r#""2KiB""#).unwrap(),
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1
        );
        assert_eq!(
            serde_json::from_str::<RegisteredPoStProof>(r#""StackedDRGWindow2KiBV1P1""#).unwrap(),
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1
        );
    }
}
