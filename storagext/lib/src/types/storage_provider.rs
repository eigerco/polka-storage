/// This module contains trait implementation for storage provider related types.
///
/// Since `storagext` declares some "duplicate" types from the runtime to be more ergonomic,
/// types imported from the runtime that have doppelgangers, should be imported using `as` and
/// prefixed with `Runtime`, making them easily distinguishable from their doppelgangers.
use std::collections::{BTreeMap, BTreeSet};

use cid::Cid;
use primitives::{
    proofs::{RegisteredPoStProof, RegisteredSealProof},
    sector::SectorNumber,
    DealId, PartitionNumber,
};

use crate::{
    runtime::{
        bounded_vec::IntoBoundedByteVec,
        runtime_types::{
            bounded_collections::{bounded_btree_set, bounded_vec},
            pallet_storage_provider::{
                fault::{
                    DeclareFaultsParams as RuntimeDeclareFaultsParams,
                    DeclareFaultsRecoveredParams as RuntimeDeclareFaultsRecoveredParams,
                    FaultDeclaration as RuntimeFaultDeclaration,
                    RecoveryDeclaration as RuntimeRecoveryDeclaration,
                },
                proofs::{
                    PoStProof as RuntimePoStProof,
                    SubmitWindowedPoStParams as RuntimeSubmitWindowedPoStParams,
                },
                sector::{
                    ProveCommitSector as RuntimeProveCommitSector,
                    SectorPreCommitInfo as RuntimeSectorPreCommitInfo,
                    TerminateSectorsParams as RuntimeTerminateSectorsParams,
                    TerminationDeclaration as RuntimeTerminationDeclaration,
                },
            },
            primitives::pallets::DeadlineState as RuntimeDeadlineState,
        },
    },
    BlockNumber,
};

// The following conversions have specific account ID types because of the subxt generation,
// the type required there is `subxt::ext::subxt_core::utils::AccountId32`, however, this type
// is not very useful on its own, it doesn't allow us to print an account ID as anything else
// other than an array of bytes, hence, we use a more generic type for the config
// `subxt::ext::sp_core::crypto::AccountId32` and convert back to the one generated by subxt.

#[derive(Clone, Debug, serde::Deserialize)]
pub struct SectorPreCommitInfo {
    /// Type of seal that was used when registering a Storage Provider.
    pub seal_proof: RegisteredSealProof,

    /// Which sector number this SP is pre-committing.
    pub sector_number: SectorNumber,

    /// Deals IDs to be activated.
    /// If any of those is invalid, the whole activation is rejected.
    pub deal_ids: Vec<DealId>,

    /// Expiration of the pre-committed sector.
    pub expiration: BlockNumber,

    /// This value is also known as `commD` or "commitment of data".
    /// Once a sector is full `commD` is produced representing the root node of all of the piece CIDs contained in the sector.
    #[serde(deserialize_with = "crate::types::deserialize_string_to_cid")]
    pub unsealed_cid: cid::Cid,

    /// This value is also known as `commR` or "commitment of replication". The terms `commR` and `sealed_cid` are interchangeable.
    /// Using sealed_cid as I think that is more descriptive.
    /// Some docs on `commR` here: <https://proto.school/verifying-storage-on-filecoin/03>
    #[serde(deserialize_with = "crate::types::deserialize_string_to_cid")]
    pub sealed_cid: cid::Cid,

    /// The blocknumber used in the porep proof.
    pub seal_randomness_height: BlockNumber,
}

impl From<SectorPreCommitInfo> for RuntimeSectorPreCommitInfo<BlockNumber> {
    fn from(value: SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: value.seal_proof,
            sector_number: value.sector_number,
            sealed_cid: value.sealed_cid.into_bounded_byte_vec(),
            deal_ids: crate::runtime::polka_storage_runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec(value.deal_ids),
            expiration: value.expiration,
            unsealed_cid: value.unsealed_cid.into_bounded_byte_vec(),
            seal_randomness_height: value.seal_randomness_height,
        }
    }
}

impl From<RuntimeSectorPreCommitInfo<BlockNumber>> for SectorPreCommitInfo {
    fn from(value: RuntimeSectorPreCommitInfo<BlockNumber>) -> Self {
        Self {
            seal_proof: value.seal_proof,
            sector_number: value.sector_number,
            sealed_cid: Cid::read_bytes(value.sealed_cid.0.as_slice())
                .expect("a proper value to have been stored on chain"),
            deal_ids: value.deal_ids.0,
            expiration: value.expiration,
            unsealed_cid: Cid::read_bytes(value.unsealed_cid.0.as_slice())
                .expect("a proper value to have been stored on chain"),
            seal_randomness_height: value.seal_randomness_height,
        }
    }
}

impl PartialEq<RuntimeSectorPreCommitInfo<BlockNumber>> for SectorPreCommitInfo {
    fn eq(&self, other: &RuntimeSectorPreCommitInfo<BlockNumber>) -> bool {
        self.deal_ids == other.deal_ids.0
            && self.expiration == other.expiration
            && self.seal_proof == other.seal_proof
            && self.sealed_cid.to_bytes() == other.sealed_cid.0
            && self.unsealed_cid.to_bytes() == other.unsealed_cid.0
            && self.sector_number == other.sector_number
            && self.seal_randomness_height == other.seal_randomness_height
    }
}

impl PartialEq<SectorPreCommitInfo> for RuntimeSectorPreCommitInfo<BlockNumber> {
    fn eq(&self, other: &SectorPreCommitInfo) -> bool {
        self.deal_ids.0 == other.deal_ids
            && self.expiration == other.expiration
            && self.seal_proof == other.seal_proof
            && self.sealed_cid.0 == other.sealed_cid.to_bytes()
            && self.unsealed_cid.0 == other.unsealed_cid.to_bytes()
            && self.sector_number == other.sector_number
            && self.seal_randomness_height == other.seal_randomness_height
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ProveCommitSector {
    /// Number of a sector that has been previously pre-committed.
    pub sector_number: SectorNumber,
    /// Raw proof bytes serialized with [`parity_scale_codec::Encode::encode`]
    /// and using [`bls12_381::Bls12`] as a curve.
    #[serde(with = "hex")]
    pub proof: Vec<u8>,
}

impl From<ProveCommitSector> for RuntimeProveCommitSector {
    fn from(value: ProveCommitSector) -> Self {
        Self {
            sector_number: value.sector_number,
            proof: value.proof.into_bounded_byte_vec(),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, serde::Deserialize)]
pub struct FaultDeclaration {
    pub deadline: u64,
    pub partition: u32,
    pub sectors: BTreeSet<SectorNumber>,
}

impl From<FaultDeclaration> for RuntimeFaultDeclaration {
    fn from(value: FaultDeclaration) -> Self {
        Self {
            deadline: value.deadline,
            partition: value.partition,
            // Converts from BTreeSet -> Vec -> BoundedBTreeSet because subxt...
            sectors: bounded_btree_set::BoundedBTreeSet(value.sectors.into_iter().collect()),
        }
    }
}

impl From<Vec<FaultDeclaration>> for RuntimeDeclareFaultsParams {
    fn from(value: Vec<FaultDeclaration>) -> Self {
        Self {
            faults: bounded_vec::BoundedVec(value.into_iter().map(Into::into).collect()),
        }
    }
}

impl PartialEq<FaultDeclaration> for RuntimeFaultDeclaration {
    fn eq(&self, other: &FaultDeclaration) -> bool {
        self.deadline == other.deadline
            && self.partition == other.partition
            && self.sectors.0.len() == other.sectors.len()
            && self
                .sectors
                .0
                .iter()
                .all(|sector| other.sectors.contains(sector))
    }
}

impl PartialEq<RuntimeFaultDeclaration> for FaultDeclaration {
    fn eq(&self, other: &RuntimeFaultDeclaration) -> bool {
        self.deadline == other.deadline
            && self.partition == other.partition
            && self.sectors.len() == other.sectors.0.len()
            && other
                .sectors
                .0
                .iter()
                .all(|sector| self.sectors.contains(sector))
    }
}

#[derive(PartialEq, Eq, Debug, Clone, serde::Deserialize)]
pub struct RecoveryDeclaration {
    pub deadline: u64,
    pub partition: u32,
    pub sectors: BTreeSet<SectorNumber>,
}

impl From<RecoveryDeclaration> for RuntimeRecoveryDeclaration {
    fn from(value: RecoveryDeclaration) -> Self {
        Self {
            deadline: value.deadline,
            partition: value.partition,
            // Converts from BTreeSet -> Vec -> BoundedBTreeSet because subxt...
            sectors: bounded_btree_set::BoundedBTreeSet(value.sectors.into_iter().collect()),
        }
    }
}

impl From<Vec<RecoveryDeclaration>> for RuntimeDeclareFaultsRecoveredParams {
    fn from(value: Vec<RecoveryDeclaration>) -> Self {
        Self {
            recoveries: bounded_vec::BoundedVec(value.into_iter().map(Into::into).collect()),
        }
    }
}

impl PartialEq<RecoveryDeclaration> for RuntimeRecoveryDeclaration {
    fn eq(&self, other: &RecoveryDeclaration) -> bool {
        self.deadline == other.deadline
            && self.partition == other.partition
            && self.sectors.0.len() == other.sectors.len()
            && self
                .sectors
                .0
                .iter()
                .all(|sector| other.sectors.contains(sector))
    }
}

impl PartialEq<RuntimeRecoveryDeclaration> for RecoveryDeclaration {
    fn eq(&self, other: &RuntimeRecoveryDeclaration) -> bool {
        self.deadline == other.deadline
            && self.partition == other.partition
            && self.sectors.len() == other.sectors.0.len()
            && other
                .sectors
                .0
                .iter()
                .all(|sector| self.sectors.contains(sector))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
    #[serde(with = "hex")]
    pub proof_bytes: Vec<u8>,
}

impl Into<RuntimePoStProof> for PoStProof {
    fn into(self) -> RuntimePoStProof {
        RuntimePoStProof {
            post_proof: self.post_proof,
            proof_bytes: self.proof_bytes.into_bounded_byte_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct SubmitWindowedPoStParams {
    pub deadline: u64,
    pub partitions: Vec<u32>,
    pub proof: PoStProof,
}

impl Into<RuntimeSubmitWindowedPoStParams> for SubmitWindowedPoStParams {
    fn into(self) -> RuntimeSubmitWindowedPoStParams {
        RuntimeSubmitWindowedPoStParams {
            deadline: self.deadline,
            partitions: bounded_vec::BoundedVec(self.partitions),
            proof: self.proof.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct TerminationDeclaration {
    pub deadline: u64,
    pub partition: u32,
    pub sectors: BTreeSet<SectorNumber>,
}

impl From<TerminationDeclaration> for RuntimeTerminationDeclaration {
    fn from(value: TerminationDeclaration) -> Self {
        Self {
            deadline: value.deadline,
            partition: value.partition,
            // Converts from BTreeSet -> Vec -> BoundedBTreeSet because subxt...
            sectors: bounded_btree_set::BoundedBTreeSet(value.sectors.into_iter().collect()),
        }
    }
}

impl From<Vec<TerminationDeclaration>> for RuntimeTerminateSectorsParams {
    fn from(value: Vec<TerminationDeclaration>) -> Self {
        Self {
            terminations: bounded_vec::BoundedVec(value.into_iter().map(Into::into).collect()),
        }
    }
}

pub struct PartitionState {
    pub sectors: BTreeSet<SectorNumber>,
}

pub struct DeadlineState {
    pub partitions: BTreeMap<PartitionNumber, PartitionState>,
}

impl From<RuntimeDeadlineState> for DeadlineState {
    fn from(value: RuntimeDeadlineState) -> Self {
        Self {
            partitions: BTreeMap::from_iter(value.partitions.0.into_iter().map(|(k, v)| {
                (
                    k,
                    PartitionState {
                        sectors: BTreeSet::from_iter(v.sectors.0.into_iter()),
                    },
                )
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr};

    use cid::Cid;
    use primitives::proofs::RegisteredPoStProof;

    use crate::{
        runtime::runtime_types::pallet_market::pallet::DealState as RuntimeDealState,
        types::{
            market::DealProposal,
            storage_provider::{
                FaultDeclaration, PoStProof, RecoveryDeclaration, SubmitWindowedPoStParams,
                TerminationDeclaration,
            },
        },
        PolkaStorageConfig,
    };

    #[test]
    fn ensure_deserialization_faults() {
        let declaration = r#"
        {
            "deadline": 0,
            "partition": 0,
            "sectors": [0, 1]
        }
        "#;
        let result: FaultDeclaration = serde_json::from_str(declaration).unwrap();
        let expected = FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors: BTreeSet::from_iter([0.into(), 1.into()].into_iter()),
        };
        assert_eq!(expected, result);
    }

    #[test]
    fn ensure_deserialization_faults_vec() {
        let declaration = r#"
        [{
            "deadline": 0,
            "partition": 0,
            "sectors": [0, 1]
        }]
        "#;
        let result: Vec<FaultDeclaration> = serde_json::from_str(declaration).unwrap();
        let expected = vec![FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors: BTreeSet::from_iter([0.into(), 1.into()].into_iter()),
        }];
        assert_eq!(expected, result);
    }

    #[test]
    fn ensure_deserialization_recoveries() {
        let declaration = r#"
        {
            "deadline": 0,
            "partition": 0,
            "sectors": [0, 1]
        }
        "#;
        let result: RecoveryDeclaration = serde_json::from_str(declaration).unwrap();
        let expected = RecoveryDeclaration {
            deadline: 0,
            partition: 0,
            sectors: BTreeSet::from_iter([0.into(), 1.into()].into_iter()),
        };
        assert_eq!(expected, result);
    }

    #[test]
    fn ensure_deserialization_recoveries_vec() {
        let declaration = r#"
        [{
            "deadline": 0,
            "partition": 0,
            "sectors": [0, 1]
        }]
        "#;
        let result: Vec<RecoveryDeclaration> = serde_json::from_str(declaration).unwrap();
        let expected = vec![RecoveryDeclaration {
            deadline: 0,
            partition: 0,
            sectors: BTreeSet::from_iter([0.into(), 1.into()].into_iter()),
        }];
        assert_eq!(expected, result);
    }

    #[test]
    fn deserialize_deal_proposal_json_object() {
        let json = r#"
        {
            "piece_cid": "bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy",
            "piece_size": 1,
            "client": "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            "provider": "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            "label": "heyyy",
            "start_block": 30,
            "end_block": 55,
            "storage_price_per_block": 1,
            "provider_collateral": 1,
            "state": "Published"
        }
        "#;
        let result_deal_proposal = serde_json::from_str::<DealProposal>(json).unwrap();

        let piece_cid =
            Cid::from_str("bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy").unwrap();
        let expect_deal_proposal = DealProposal {
            piece_cid,
            piece_size: 1,
            client: <PolkaStorageConfig as subxt::Config>::AccountId::from_str(
                "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            )
            .unwrap(),
            provider: <PolkaStorageConfig as subxt::Config>::AccountId::from_str(
                "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            )
            .unwrap(),
            label: "heyyy".to_string(),
            start_block: 30,
            end_block: 55,
            storage_price_per_block: 1,
            provider_collateral: 1,
            state: RuntimeDealState::Published,
        };

        assert_eq!(result_deal_proposal, expect_deal_proposal);
    }

    #[test]
    fn ensure_serde_for_post_proof() {
        let proof = serde_json::from_str::<PoStProof>(
            r#"{
                "post_proof": "2KiB",
                "proof_bytes": "1234567890"
            }"#,
        )
        .unwrap();
        assert_eq!(
            proof,
            PoStProof {
                post_proof: RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                proof_bytes: vec![0x12u8, 0x34, 0x56, 0x78, 0x90]
            }
        );
    }

    #[test]
    fn ensure_serde_for_submit_windowed_post_params() {
        let proof = serde_json::from_str::<SubmitWindowedPoStParams>(
            r#"{
                "deadline": 10,
                "partitions": [10],
                "proof": {
                    "post_proof": "2KiB",
                    "proof_bytes": "1234567890"
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            proof,
            SubmitWindowedPoStParams {
                deadline: 10,
                partitions: vec![10],
                proof: PoStProof {
                    post_proof: RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                    proof_bytes: vec![0x12u8, 0x34, 0x56, 0x78, 0x90]
                }
            }
        );
    }

    #[test]
    fn ensure_serde_for_termination_declaration() {
        let termination = serde_json::from_str::<Vec<TerminationDeclaration>>(
            r#"[{
                "deadline": 69,
                "partition": 420,
                "sectors": [1, 2]
            }]"#,
        )
        .unwrap();
        assert_eq!(
            termination,
            vec![TerminationDeclaration {
                deadline: 69,
                partition: 420,
                sectors: BTreeSet::from([1.into(), 2.into()]),
            }]
        )
    }
}
