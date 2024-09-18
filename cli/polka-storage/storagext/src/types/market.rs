use cid::Cid;
use codec::Encode;
use subxt::{ext::sp_runtime::MultiSignature, tx::Signer, utils::Static};

use crate::{
    runtime::{
        bounded_vec::IntoBoundedByteVec,
        runtime_types::pallet_market::pallet::{
            ClientDealProposal as RuntimeClientDealProposal, DealProposal as RuntimeDealProposal,
            DealState as RuntimeDealState,
        },
    },
    BlockNumber, Currency, PolkaStorageConfig,
};

/// Doppelganger of `RuntimeDealProposal` but with more ergonomic types and no generics.
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct DealProposal {
    #[serde(deserialize_with = "crate::types::deserialize_string_to_cid")]
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub client: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub provider: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: RuntimeDealState<BlockNumber>,
}

impl From<DealProposal>
    for RuntimeDealProposal<subxt::ext::subxt_core::utils::AccountId32, Currency, BlockNumber>
{
    fn from(value: DealProposal) -> Self {
        Self {
            piece_cid: value.piece_cid.into_bounded_byte_vec(),
            piece_size: value.piece_size,
            client: value.client.into(),
            provider: value.provider.into(),
            label: value.label.into_bounded_byte_vec(),
            start_block: value.start_block,
            end_block: value.end_block,
            storage_price_per_block: value.storage_price_per_block,
            provider_collateral: value.provider_collateral,
            state: value.state,
        }
    }
}

impl DealProposal {
    /// Consumes the [`DealProposal`], signs it using the provided keypair
    /// and returns a deal proposal ready to be submitted.
    pub(crate) fn sign<Keypair>(
        self,
        keypair: &Keypair,
    ) -> RuntimeClientDealProposal<
        subxt::ext::subxt_core::utils::AccountId32,
        Currency,
        BlockNumber,
        Static<MultiSignature>,
    >
    where
        Keypair: Signer<PolkaStorageConfig>,
        Self: Into<
            RuntimeDealProposal<subxt::ext::subxt_core::utils::AccountId32, Currency, BlockNumber>,
        >,
    {
        let proposal: RuntimeDealProposal<_, _, _> = self.into();
        let encoded = &proposal.encode();
        tracing::trace!("deal_proposal: encoded proposal: {}", hex::encode(&encoded));
        let client_signature = Static(keypair.sign(encoded));

        RuntimeClientDealProposal {
            proposal,
            client_signature,
        }
    }
}
