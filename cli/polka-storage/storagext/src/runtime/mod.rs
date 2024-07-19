//! This module covers the Runtime API extracted from SCALE-encoded runtime and extra goodies
//! to interface with the runtime.
//!
//! This module wasn't designed to be exposed to the final user of the crate.

mod bounded_vec;

#[subxt::subxt(
    runtime_metadata_path = "../../artifacts/metadata.scale",
    substitute_type(
        path = "sp_runtime::MultiSignature",
        with = "::subxt::utils::Static<::frame_support::sp_runtime::MultiSignature>"
    )
)]
mod polka_storage_runtime {}

use frame_support::sp_runtime::MultiSignature;
use subxt::{config::polkadot::AccountId32, utils::Static};

// Using self keeps the import separate from the others
pub use self::polka_storage_runtime::*;
use self::{
    bounded_vec::IntoBoundedByteVec, runtime_types::pallet_market::pallet as market_pallet_types,
};

type BlockNumber = u32;

impl From<crate::ActiveDealState> for market_pallet_types::ActiveDealState<BlockNumber> {
    fn from(value: crate::ActiveDealState) -> Self {
        Self {
            sector_number: value.sector_number,
            sector_start_block: value.sector_start_block,
            last_updated_block: value.last_updated_block,
            slash_block: value.slash_block,
        }
    }
}

impl From<crate::DealState> for market_pallet_types::DealState<BlockNumber> {
    fn from(value: crate::DealState) -> Self {
        match value {
            crate::DealState::Active(value) => market_pallet_types::DealState::Active(
                market_pallet_types::ActiveDealState::from(value),
            ),
            crate::DealState::Published => market_pallet_types::DealState::Published,
        }
    }
}

impl From<crate::DealProposal>
    for market_pallet_types::DealProposal<AccountId32, u128, BlockNumber>
{
    fn from(value: crate::DealProposal) -> Self {
        Self {
            piece_cid: value.piece_cid.0.into_bounded_byte_vec(),
            piece_size: value.piece_size,
            client: AccountId32::from(value.client),
            provider: AccountId32::from(value.provider),
            label: value.label.into_bounded_byte_vec(),
            start_block: value.start_block,
            end_block: value.end_block,
            storage_price_per_block: value.storage_price_per_block,
            provider_collateral: value.provider_collateral,
            state: market_pallet_types::DealState::from(value.state),
        }
    }
}

impl From<crate::ClientDealProposal>
    for market_pallet_types::ClientDealProposal<
        AccountId32,
        u128,
        BlockNumber,
        Static<MultiSignature>,
    >
{
    fn from(value: crate::ClientDealProposal) -> Self {
        Self {
            proposal: market_pallet_types::DealProposal::from(value.proposal),
            client_signature: Static(value.client.into()),
        }
    }
}
