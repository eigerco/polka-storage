pub mod market;
pub mod runtime;
pub mod storage_provider;

use cid::Cid;
use codec::Encode;
use frame_support::CloneNoBound;
use primitives_proofs::{DealId, RegisteredPoStProof, RegisteredSealProof, SectorNumber};
use subxt::{self, ext::sp_runtime::MultiSignature, tx::Signer, utils::Static};

use crate::runtime::bounded_vec::IntoBoundedByteVec;
pub use crate::runtime::runtime_types::{
    pallet_market::{
        pallet as market_pallet_types,
        pallet::{ActiveDealState, DealState},
    },
    primitives_proofs::types as primitives_proofs_types,
};

/// Currency as specified by the SCALE-encoded runtime.
pub type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
pub type BlockNumber = u64;

/// Parachain configuration for subxt.
pub enum PolkaStorageConfig {}

// Types are fully qualified ON PURPOSE!
// It's not fun to find out where in your config a type comes from subxt or frame_support
// going up and down, in and out the files, this helps!
impl subxt::Config for PolkaStorageConfig {
    type Hash = subxt::utils::H256;
    type AccountId = subxt::ext::sp_core::crypto::AccountId32;
    type Address = subxt::config::polkadot::MultiAddress<Self::AccountId, u32>;
    type Signature = subxt::ext::sp_runtime::MultiSignature;
    type Hasher = subxt::config::substrate::BlakeTwo256;
    type Header = subxt::config::substrate::SubstrateHeader<
        BlockNumber,
        subxt::config::substrate::BlakeTwo256,
    >;
    type ExtrinsicParams = subxt::config::DefaultExtrinsicParams<Self>;
    type AssetId = u32;
}

// We need this type because of the CID & label ergonomics.
#[derive(CloneNoBound)]
pub struct DealProposal {
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub client: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub provider: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: market_pallet_types::DealState<BlockNumber>,
}

// The following conversions have specific account ID types because of the subxt generation,
// the type required there is `subxt::ext::subxt_core::utils::AccountId32`, however, this type
// is not very useful on its own, it doesn't allow us to print an account ID as anything else
// other than an array of bytes, hence, we use a more generic type for the config
// `subxt::ext::sp_core::crypto::AccountId32` and convert back to the one generated by subxt.

impl From<DealProposal>
    for market_pallet_types::DealProposal<
        subxt::ext::subxt_core::utils::AccountId32,
        Currency,
        BlockNumber,
    >
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
    fn sign<Keypair>(
        self,
        keypair: &Keypair,
    ) -> market_pallet_types::ClientDealProposal<
        subxt::ext::subxt_core::utils::AccountId32,
        Currency,
        BlockNumber,
        Static<MultiSignature>,
    >
    where
        Keypair: Signer<PolkaStorageConfig>,
        Self: Into<
            market_pallet_types::DealProposal<
                subxt::ext::subxt_core::utils::AccountId32,
                Currency,
                BlockNumber,
            >,
        >,
    {
        let proposal: market_pallet_types::DealProposal<_, _, _> = self.into();
        let encoded = &proposal.encode();
        tracing::trace!("deal_proposal: encoded proposal: {}", hex::encode(&encoded));
        let client_signature = Static(keypair.sign(encoded));

        market_pallet_types::ClientDealProposal {
            proposal,
            client_signature,
        }
    }
}

#[derive(CloneNoBound)]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub deal_ids: Vec<DealId>,
    pub expiration: BlockNumber,
    pub unsealed_cid: Cid,
}

impl From<SectorPreCommitInfo>
    for runtime::runtime_types::pallet_storage_provider::sector::SectorPreCommitInfo<BlockNumber>
{
    fn from(value: SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: value.seal_proof,
            sector_number: value.sector_number,
            sealed_cid: value.sealed_cid.into_bounded_byte_vec(),
            deal_ids: crate::runtime::polka_storage_runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec(value.deal_ids),
            expiration: value.expiration,
            unsealed_cid: value.unsealed_cid.into_bounded_byte_vec(),
        }
    }
}

#[derive(CloneNoBound)]
pub struct ProveCommitSector {
    pub sector_number: SectorNumber,
    pub proof: Vec<u8>,
}

impl From<ProveCommitSector>
    for runtime::runtime_types::pallet_storage_provider::sector::ProveCommitSector
{
    fn from(value: ProveCommitSector) -> Self {
        Self {
            sector_number: value.sector_number,
            proof: value.proof.into_bounded_byte_vec(),
        }
    }
}
