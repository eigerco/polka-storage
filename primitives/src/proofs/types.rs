use core::{fmt::Display, marker::PhantomData};

use codec::{Decode, Encode, MaxEncodedLen};
use scale_decode::{
    visitor::{self},
    DecodeAsType, ToString, TypeResolver, Visitor,
};
use scale_encode::EncodeAsType;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

use crate::sector::SectorSize;

pub type DealId = u64;

/// Byte representation of the entity that was signing the proof.
/// It must match the ProverId used for Proving.
pub type ProverId = [u8; 32];

/// Byte representation of randomness seed, it's used for challenge generation.
pub type Ticket = [u8; 32];

#[allow(non_camel_case_types)]
#[derive(
    Debug, Decode, Encode, DecodeAsType, EncodeAsType, TypeInfo, Eq, PartialEq, Clone, Copy,
)]
#[cfg_attr(feature = "clap", derive(::clap::ValueEnum))]
#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[codec(crate = ::codec)]
#[decode_as_type(crate_path = "::scale_decode")]
#[encode_as_type(crate_path = "::scale_encode")]
/// References:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L18>
pub enum RegisteredSealProof {
    #[cfg_attr(feature = "clap", clap(name = "2KiB"))]
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

    /// Proof size in bytes for each SealProof type.
    ///
    /// Reference:
    /// * <https://github.com/filecoin-project/ref-fvm/blob/b72a51084f3b65f8bd41f4a9a733d43bb4b1d6f7/shared/src/sector/registered_proof.rs#L90>
    pub fn proof_size(self) -> usize {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => 192,
        }
    }
}

/// Proof of Spacetime type, indicating version and sector size of the proof.
#[derive(
    Debug, Decode, Encode, DecodeAsType, EncodeAsType, TypeInfo, PartialEq, Eq, Clone, Copy,
)]
#[cfg_attr(feature = "clap", derive(::clap::ValueEnum))]
#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[codec(crate = ::codec)]
#[decode_as_type(crate_path = "::scale_decode")]
#[encode_as_type(crate_path = "::scale_encode")]
pub enum RegisteredPoStProof {
    #[cfg_attr(feature = "clap", clap(name = "2KiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "2KiB"))]
    StackedDRGWindow2KiBV1P1,
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(&self) -> SectorSize {
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => SectorSize::_2KiB,
        }
    }

    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(&self) -> u64 {
        // Resolve to post proof and then compute size from that.
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => 2,
        }
    }

    /// Number of sectors challenged in a replica.
    ///
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L102>
    pub fn sector_count(&self) -> usize {
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
    use super::{RegisteredPoStProof, RegisteredSealProof};

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
