///! Runtime API extracted from SCALE-encoded runtime.

#[subxt::subxt(runtime_metadata_path = "../../artifacts/metadata.scale")]
mod polka_storage_runtime {}

use cid::Cid;
use frame_support::sp_runtime::AccountId32;
pub use polka_storage_runtime::*;
use runtime_types::pallet_market::pallet::ClientDealProposal;
use subxt::utils::MultiSignature;

impl From<crate::DealProposal>
    for runtime_types::pallet_market::pallet::DealProposal<AccountId32, u128, u32>
{
    fn from(value: crate::DealProposal) -> Self {
        Self {
            piece_cid: value.piece_cid.0.into_bounded_byte_vec(),
            piece_size: value.piece_size,
            client: value.client,
            provider: value.provider,
            label: value.label.into_bounded_byte_vec(),
            start_block: value.start_block,
            end_block: value.end_block,
            storage_price_per_block: value.storage_price_per_block,
            provider_collateral: value.provider_collateral,
            state: value.state,
        }
    }
}

impl From<crate::ClientDealProposal>
    for ClientDealProposal<
        AccountId32,
        u128,
        u32,
        MultiSignature, // hmmmm
    >
{
    fn from(value: crate::ClientDealProposal) -> Self {
        Self {
            proposal: value.proposal,
            client_signature: value.client.into(),
        }
    }
}

trait IntoBoundedByteVec {
    fn into_bounded_byte_vec(
        self,
    ) -> runtime_types::bounded_collections::bounded_vec::BoundedVec<u8>;
}

impl IntoBoundedByteVec for Cid {
    fn into_bounded_byte_vec(
        self,
    ) -> runtime_types::bounded_collections::bounded_vec::BoundedVec<u8> {
        runtime_types::bounded_collections::bounded_vec::BoundedVec(self.to_bytes())
    }
}

impl IntoBoundedByteVec for String {
    fn into_bounded_byte_vec(
        self,
    ) -> runtime_types::bounded_collections::bounded_vec::BoundedVec<u8> {
        runtime_types::bounded_collections::bounded_vec::BoundedVec(self.into_bytes())
    }
}
