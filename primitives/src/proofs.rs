use codec::{Decode, Encode, MaxEncodedLen};
use scale_decode::DecodeAsType;
use scale_encode::EncodeAsType;
use scale_info::TypeInfo;
use sp_core::blake2_256;

use crate::{commitment::RawCommitment, sector::SectorSize};

/// Byte representation of the entity that was signing the proof.
/// It must match the ProverId used for Proving.
pub type ProverId = [u8; 32];

/// Byte representation of randomness seed, it's used for challenge generation.
pub type Ticket = [u8; 32];

/// Derives a unique prover ID for a given account.
///
/// The function takes an `AccountId` and generates a 32-byte array that serves
/// as a unique identifier for the prover associated with that account. The
/// prover ID is derived using the Blake2 hash of the encoded account ID.
pub fn derive_prover_id<AccountId>(account_id: AccountId) -> [u8; 32]
where
    AccountId: Encode,
{
    let encoded = account_id.encode();
    let mut encoded = blake2_256(&encoded);

    // Necessary to be a valid bls12 381 element.
    encoded[31] &= 0x3f;
    encoded
}

/// The minimal information required about a replica, in order to be able to verify
/// a PoSt over it.
#[derive(Clone, core::fmt::Debug, PartialEq, Eq)]
pub struct PublicReplicaInfo {
    /// The replica commitment.
    pub comm_r: RawCommitment,
}

#[allow(non_camel_case_types)]
#[derive(
    Debug,
    Decode,
    Encode,
    DecodeAsType,
    EncodeAsType,
    TypeInfo,
    Eq,
    PartialEq,
    Clone,
    Copy,
    Hash,
    MaxEncodedLen,
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
    #[cfg_attr(feature = "clap", clap(name = "8MiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "8MiB"))]
    StackedDRG8MiBV1,
    #[cfg_attr(feature = "clap", clap(name = "512MiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "512MiB"))]
    StackedDRG512MiBV1,
    #[cfg_attr(feature = "clap", clap(name = "1GiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "1GiB"))]
    StackedDRG1GiBV1,
}

impl RegisteredSealProof {
    pub fn sector_size(&self) -> SectorSize {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => SectorSize::_2KiB,
            RegisteredSealProof::StackedDRG8MiBV1 => SectorSize::_8MiB,
            RegisteredSealProof::StackedDRG512MiBV1 => SectorSize::_512MiB,
            RegisteredSealProof::StackedDRG1GiBV1 => SectorSize::_1GiB,
        }
    }

    /// Produces the windowed PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_window_post_proof(&self) -> RegisteredPoStProof {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => {
                RegisteredPoStProof::StackedDRGWindow2KiBV1P1
            }
            RegisteredSealProof::StackedDRG8MiBV1 => RegisteredPoStProof::StackedDRGWindow8MiBV1,
            RegisteredSealProof::StackedDRG512MiBV1 => {
                RegisteredPoStProof::StackedDRGWindow512MiBV1
            }
            RegisteredSealProof::StackedDRG1GiBV1 => RegisteredPoStProof::StackedDRGWindow1GiBV1,
        }
    }

    /// Proof size in bytes for each SealProof type.
    ///
    /// Reference:
    /// * <https://github.com/filecoin-project/ref-fvm/blob/b72a51084f3b65f8bd41f4a9a733d43bb4b1d6f7/shared/src/sector/registered_proof.rs#L90>
    pub fn proof_size(self) -> usize {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => 192,
            RegisteredSealProof::StackedDRG8MiBV1 => 192,
            RegisteredSealProof::StackedDRG512MiBV1 => 192,
            RegisteredSealProof::StackedDRG1GiBV1 => 192,
        }
    }

    /// Byte identifier used to generate the replica.
    /// References:
    /// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L292>
    pub fn porep_id(&self) -> [u8; 32] {
        let mut porep_id = [0; 32];
        let registered_proof_id = self.proof_id();
        let n = self.nonce();

        porep_id[0..8].copy_from_slice(&registered_proof_id.to_le_bytes());
        porep_id[8..16].copy_from_slice(&n.to_le_bytes());
        porep_id
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L283C1-L302C6>
    fn nonce(&self) -> u64 {
        #[allow(clippy::match_single_binding)]
        match self {
            // If we ever need to change the nonce for any given RegisteredSealProof, match it here.
            _ => 0,
        }
    }

    /// Reference:
    /// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L52>
    fn proof_id(&self) -> u64 {
        match self {
            RegisteredSealProof::StackedDRG2KiBV1P1 => 0,
            RegisteredSealProof::StackedDRG8MiBV1 => 1,
            RegisteredSealProof::StackedDRG512MiBV1 => 2,
            // NOTE(@th7nder,31/01/2025): there is no such thing as 1GiB in registered in FC
            RegisteredSealProof::StackedDRG1GiBV1 => 20,
        }
    }

    /// Returns [`StackedDRG2KiBV1P1`](RegisteredSealProof::StackedDRG2KiBV1P1).
    // NOTE(@jmg-duarte,14/01/2025): wanted to avoid setting a default to use in serde
    // this is the alternative
    #[allow(non_snake_case)]
    pub const fn _2KiB() -> Self {
        Self::StackedDRG2KiBV1P1
    }
}

/// Proof of Spacetime type, indicating version and sector size of the proof.
#[derive(
    Debug,
    Decode,
    Encode,
    DecodeAsType,
    EncodeAsType,
    TypeInfo,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Hash,
    MaxEncodedLen,
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
    #[cfg_attr(feature = "clap", clap(name = "8MiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "8MiB"))]
    StackedDRGWindow8MiBV1,
    #[cfg_attr(feature = "clap", clap(name = "512MiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "512MiB"))]
    StackedDRGWindow512MiBV1,
    #[cfg_attr(feature = "clap", clap(name = "1GiB"))]
    #[cfg_attr(feature = "serde", serde(alias = "1GiB"))]
    StackedDRGWindow1GiBV1,
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(&self) -> SectorSize {
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => SectorSize::_2KiB,
            RegisteredPoStProof::StackedDRGWindow8MiBV1 => SectorSize::_8MiB,
            RegisteredPoStProof::StackedDRGWindow512MiBV1 => SectorSize::_512MiB,
            RegisteredPoStProof::StackedDRGWindow1GiBV1 => SectorSize::_1GiB,
        }
    }

    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(&self) -> u64 {
        // Resolve to post proof and then compute size from that.
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => 2,
            RegisteredPoStProof::StackedDRGWindow8MiBV1 => 2,
            RegisteredPoStProof::StackedDRGWindow512MiBV1 => 2,
            RegisteredPoStProof::StackedDRGWindow1GiBV1 => 2,
        }
    }

    /// Number of sectors challenged in a replica.
    ///
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L102>
    pub fn sector_count(&self) -> usize {
        match self {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => 2,
            RegisteredPoStProof::StackedDRGWindow8MiBV1 => 2,
            RegisteredPoStProof::StackedDRGWindow512MiBV1 => 2,
            RegisteredPoStProof::StackedDRGWindow1GiBV1 => 2,
        }
    }

    /// Returns [`StackedDRGWindow2KiBV1P1`](RegisteredPoStProof::StackedDRGWindow2KiBV1P1).
    // NOTE(@jmg-duarte,14/01/2025): wanted to avoid setting a default to use in serde
    // this is the alternative
    #[allow(non_snake_case)]
    pub const fn _2KiB() -> Self {
        Self::StackedDRGWindow2KiBV1P1
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

#[cfg(feature = "testing")]
pub mod testing {
    use sp_core::ConstU32;
    use sp_runtime::{BoundedBTreeMap, BoundedVec};

    use crate::{
        pallets::ProofVerification,
        proofs::{
            ProverId, PublicReplicaInfo, RawCommitment, RegisteredPoStProof, RegisteredSealProof,
            Ticket,
        },
        sector::SectorNumber,
        MAX_POST_PROOF_BYTES, MAX_PROOFS_PER_BLOCK, MAX_REPLICAS_PER_BLOCK, MAX_SEAL_PROOF_BYTES,
    };

    /// A sentinel value for an invalid proof, everything else will be considered valid.
    pub const INVALID_PROOF: [u8; 2] = [0xd, 0xe];

    /// This is dummy proofs pallet implementation.
    /// All PoRep proofs are accepted as valid.
    /// All PoSt proofs are accepted as valid unless first of them is [`INVALID_PROOF`].
    pub struct DummyProofsVerification;

    impl ProofVerification for DummyProofsVerification {
        fn verify_porep(
            _prover_id: ProverId,
            _seal_proof: RegisteredSealProof,
            _comm_r: RawCommitment,
            _comm_d: RawCommitment,
            _sector: SectorNumber,
            _ticket: Ticket,
            _seed: Ticket,
            _proofs: BoundedVec<
                BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
                ConstU32<MAX_PROOFS_PER_BLOCK>,
            >,
        ) -> sp_runtime::DispatchResult {
            Ok(())
        }

        fn verify_post(
            _post_type: RegisteredPoStProof,
            _randomness: Ticket,
            _replicas: BoundedBTreeMap<
                SectorNumber,
                PublicReplicaInfo,
                ConstU32<MAX_REPLICAS_PER_BLOCK>,
            >,
            proofs: BoundedVec<
                BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
                ConstU32<MAX_PROOFS_PER_BLOCK>,
            >,
        ) -> sp_runtime::DispatchResult {
            if *proofs[0] == INVALID_PROOF {
                return Err(sp_runtime::DispatchError::Other("invalid proof"));
            }
            Ok(())
        }
    }
}
