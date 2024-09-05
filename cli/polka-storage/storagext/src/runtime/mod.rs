//! This module covers the Runtime API extracted from SCALE-encoded runtime and extra goodies
//! to interface with the runtime.
//!
//! This module wasn't designed to be exposed to the final user of the crate.

pub mod bounded_vec;
pub(crate) mod client;
pub mod display;

// NOTE: you'll need to reload the window if the underlying SCALE file changes
// https://github.com/rust-lang/rust-analyzer/issues/10719
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
    substitute_type(
        path = "primitives_proofs::types::SectorSize",
        with = "::primitives_proofs::SectorSize",
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
        path = "pallet_market::pallet::BalanceEntry",
        derive = "::serde::Serialize"
    ),
    derive_for_type(path = "pallet_market::pallet::Event", derive = "::serde::Serialize"),
    derive_for_type(
        path = "pallet_storage_provider::pallet::Event",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "bounded_collections::bounded_vec::BoundedVec",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "bounded_collections::bounded_btree_set::BoundedBTreeSet",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "pallet_market::pallet::SettledDealData",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "pallet_market::pallet::DealSettlementError",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "pallet_storage_provider::storage_provider::StorageProviderInfo",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "pallet_storage_provider::sector::SectorPreCommitInfo",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "pallet_storage_provider::fault::FaultDeclaration",
        derive = "::serde::Serialize"
    ),
    derive_for_type(
        path = "pallet_storage_provider::fault::RecoveryDeclaration",
        derive = "::serde::Serialize"
    )
)]
mod polka_storage_runtime {}

// Using self keeps the import separate from the others
pub use client::SubmissionResult;

pub use self::polka_storage_runtime::*;

#[cfg(test)]
mod test {
    use crate::{ActiveDealState, DealState};

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
}
