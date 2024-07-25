//! This module covers the Runtime API extracted from SCALE-encoded runtime and extra goodies
//! to interface with the runtime.
//!
//! This module wasn't designed to be exposed to the final user of the crate.

pub(crate) mod bounded_vec;
pub(crate) mod client;

#[subxt::subxt(
    runtime_metadata_path = "../../artifacts/metadata.scale",
    substitute_type(
        path = "sp_runtime::MultiSignature",
        with = "::subxt::utils::Static<::subxt::ext::sp_runtime::MultiSignature>"
    ),
    derive_for_all_types = "Clone, PartialEq, Eq",
    derive_for_type(
        path = "pallet_market::pallet::ActiveDealState",
        derive = "::serde::Deserialize"
    ),
    derive_for_type(
        path = "pallet_market::pallet::DealState",
        derive = "::serde::Deserialize"
    ),
    derive_for_type(
        path = "primitives_proofs::types::RegisteredSealProof",
        derive = "::serde::Deserialize"
    )
)]
mod polka_storage_runtime {}

// Using self keeps the import separate from the others
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
