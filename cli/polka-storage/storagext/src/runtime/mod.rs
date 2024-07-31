//! This module covers the Runtime API extracted from SCALE-encoded runtime and extra goodies
//! to interface with the runtime.
//!
//! This module wasn't designed to be exposed to the final user of the crate.

pub(crate) mod bounded_vec;
pub(crate) mod client;

#[subxt::subxt(
    runtime_metadata_path = "../../artifacts/metadata.scale",
    derive_for_all_types = "Clone, PartialEq, Eq",
    substitute_type(
        path = "sp_runtime::MultiSignature",
        with = "::subxt::utils::Static<::subxt::ext::sp_runtime::MultiSignature>"
    ),
    substitute_type(
        path = "primitives_proofs::types::RegisteredSealProof",
        with = "::primitives_proofs::RegisteredSealProof",
    ),
    substitute_type(
        path = "primitives_proofs::types::RegisteredPoStProof",
        with = "::primitives_proofs::RegisteredPoStProof",
    ),
    derive_for_type(
        path = "pallet_market::pallet::ActiveDealState",
        derive = "::serde::Deserialize"
    ),
    derive_for_type(
        path = "pallet_market::pallet::DealState",
        derive = "::serde::Deserialize"
    ),
    derive_for_type(
        path = "pallet_storage_provider::proofs::SubmitWindowedPoStParams",
        derive = "::serde::Deserialize"
    )
)]
mod polka_storage_runtime {}

// Using self keeps the import separate from the others
pub use self::polka_storage_runtime::*;
use self::runtime_types::pallet_storage_provider as storage_provider_types;
use crate::runtime::bounded_vec::IntoBoundedByteVec;

// Necessary because the proof bytes are a `BoundedVec` which doesn't implement serde::Deserialize
struct PoStProofVisitor;

impl<'de> serde::de::Visitor<'de> for PoStProofVisitor {
    type Value = storage_provider_types::proofs::PoStProof;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a mapping with keys \"post_proof\" and \"proof_bytes\".")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        const EXPECTED_FIELDS: &[&str] = &["post_proof", "proof_bytes"];

        #[derive(serde::Deserialize)]
        struct HexVec(#[serde(with = "hex")] Vec<u8>);

        let mut post_proof = None;
        let mut proof_bytes = None;

        // Need to explicitly read the next_key as a String
        // https://github.com/serde-rs/serde/issues/1009#issuecomment-320125424
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "post_proof" => {
                    if post_proof.is_none() {
                        post_proof = Some(map.next_value()?);
                    } else {
                        return Err(serde::de::Error::duplicate_field("post_proof"));
                    }
                }
                "proof_bytes" => {
                    if proof_bytes.is_none() {
                        proof_bytes = Some(map.next_value::<HexVec>()?);
                    } else {
                        return Err(serde::de::Error::duplicate_field("proof_bytes"));
                    }
                }
                other => {
                    return Err(serde::de::Error::unknown_field(other, EXPECTED_FIELDS));
                }
            }
        }

        if post_proof.is_none() {
            return Err(serde::de::Error::missing_field("post_proof"));
        }

        if proof_bytes.is_none() {
            return Err(serde::de::Error::missing_field("proof_bytes"));
        }

        return Ok(storage_provider_types::proofs::PoStProof {
            post_proof: post_proof.expect("value should have been checked before"),
            proof_bytes: proof_bytes
                .expect("value should have been checked before")
                .0
                .into_bounded_byte_vec(),
        });
    }
}

impl<'de> serde::Deserialize<'de> for storage_provider_types::proofs::PoStProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(PoStProofVisitor)
    }
}

#[cfg(test)]
mod test {
    use primitives_proofs::RegisteredPoStProof;

    use super::runtime_types::pallet_storage_provider::proofs::{
        PoStProof, SubmitWindowedPoStParams,
    };
    use crate::{
        runtime::bounded_vec::IntoBoundedByteVec, ActiveDealState, BlockNumber, DealState,
    };

    #[test]
    fn ensure_serde_for_active_deal_state() {
        let active_deal_state = serde_json::from_str::<ActiveDealState<u64>>(
            r#"{
                "sector_number": 1,
                "sector_start_block": 10,
                "last_updated_block": 20,
                "slash_block": null
            }"#,
        )
        .unwrap();

        assert_eq!(active_deal_state.sector_number, 1);
        assert_eq!(active_deal_state.sector_start_block, 10);
        assert_eq!(active_deal_state.last_updated_block, Some(20));
        assert_eq!(active_deal_state.slash_block, None);
    }

    #[test]
    fn ensure_serde_for_deal_state_published() {
        let deal_state = serde_json::from_str::<DealState<u64>>(r#""Published""#).unwrap();

        assert_eq!(deal_state, DealState::Published);
    }

    #[test]
    fn ensure_serde_for_deal_state_active() {
        let deal_state = serde_json::from_str::<DealState<u64>>(
            r#"{
                "Active": {
                    "sector_number": 1,
                    "sector_start_block": 10,
                    "last_updated_block": 20,
                    "slash_block": null
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            deal_state,
            DealState::Active(ActiveDealState {
                sector_number: 1,
                sector_start_block: 10,
                last_updated_block: Some(20),
                slash_block: None
            })
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
                proof_bytes: vec![0x12u8, 0x34, 0x56, 0x78, 0x90].into_bounded_byte_vec()
            }
        );
    }

    #[test]
    fn ensure_serde_for_submit_windowed_post_params() {
        let proof = serde_json::from_str::<SubmitWindowedPoStParams<BlockNumber>>(
            r#"{
                "deadline": 10,
                "partition": 10,
                "chain_commit_block": 1,
                "proof": {
                    "post_proof": "2KiB",
                    "proof_bytes": "1234567890"
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            proof,
            SubmitWindowedPoStParams::<BlockNumber> {
                deadline: 10,
                partition: 10,
                chain_commit_block: 1,
                proof: PoStProof {
                    post_proof: RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                    proof_bytes: vec![0x12u8, 0x34, 0x56, 0x78, 0x90].into_bounded_byte_vec()
                }
            }
        );
    }
}
