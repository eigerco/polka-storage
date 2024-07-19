use cid::Cid;
use codec::Encode;
use frame_support::CloneNoBound;
use subxt::{tx::Signer, utils::Static};

pub mod market;
pub mod runtime;

use crate::runtime::bounded_vec::IntoBoundedByteVec;
pub use crate::runtime::runtime_types::pallet_market::{
    pallet as market_pallet_types,
    pallet::{ActiveDealState, DealState},
};

/// Currency as specified by the SCALE-encoded runtime.
type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
type BlockNumber = u32;

type AccountIndex = u32;

pub enum PolkaStorageConfig {}

// Types are fully qualified ON PURPOSE!
// It's not fun to find out where in your config a type comes from subxt or frame_support
// going up and down, in and out the files, this helps!
impl subxt::Config for PolkaStorageConfig {
    type Hash = subxt::utils::H256;
    type AccountId = subxt::config::polkadot::AccountId32;
    type Address = subxt::config::polkadot::MultiAddress<Self::AccountId, AccountIndex>;
    type Signature = frame_support::sp_runtime::MultiSignature;
    type Hasher = subxt::config::substrate::BlakeTwo256;
    type Header =
        subxt::config::substrate::SubstrateHeader<u32, subxt::config::substrate::BlakeTwo256>;
    type ExtrinsicParams = subxt::config::DefaultExtrinsicParams<Self>;
    type AssetId = u32;
}

// We need this type because of the CID & label ergonomics.
#[derive(CloneNoBound)]
pub struct DealProposal<Config>
where
    Config: subxt::Config,
{
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub client: Config::AccountId,
    pub provider: Config::AccountId,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: DealState<BlockNumber>,
}

impl<Config> From<DealProposal<Config>>
    for market_pallet_types::DealProposal<Config::AccountId, Currency, BlockNumber>
where
    Config: subxt::Config,
{
    fn from(value: DealProposal<Config>) -> Self {
        Self {
            piece_cid: value.piece_cid.into_bounded_byte_vec(),
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

impl<Config> DealProposal<Config>
where
    Config: subxt::Config,
{
    fn sign<Keypair>(self, keypair: &Keypair) -> ClientDealProposal<Config>
    where
        Keypair: Signer<Config>,
        Self: Into<market_pallet_types::DealProposal<Config::AccountId, Currency, BlockNumber>>,
    {
        let market_deal_proposal: market_pallet_types::DealProposal<_, _, _> = self.clone().into();
        let encoded_deal_proposal = market_deal_proposal.encode();

        ClientDealProposal {
            proposal: self,
            client: keypair.sign(&encoded_deal_proposal),
        }
    }
}

pub struct ClientDealProposal<Config>
where
    Config: subxt::Config,
{
    pub proposal: DealProposal<Config>,
    pub client: Config::Signature,
}

impl<Config> From<ClientDealProposal<Config>>
    for market_pallet_types::ClientDealProposal<
        Config::AccountId,
        Currency,
        BlockNumber,
        Static<Config::Signature>,
    >
where
    Config: subxt::Config,
{
    fn from(value: ClientDealProposal<Config>) -> Self {
        Self {
            proposal: market_pallet_types::DealProposal::from(value.proposal),
            client_signature: Static(value.client),
        }
    }
}
