use cid::Cid;
use codec::Encode;
use frame_support::sp_runtime::{traits::BlakeTwo256, MultiAddress, MultiSignature};
use subxt::{
    config::{polkadot::AccountId32, substrate::SubstrateHeader, DefaultExtrinsicParams},
    tx::Signer,
    utils::H256,
    Config,
};

pub mod market;
pub mod runtime;

pub enum PolkaStorageConfig {}

type AccountIndex = u32;

// Types are fully qualified ON PURPOSE!
// It's not fun to find out where in your config a type comes from subxt or frame_support
// going up and down, in and out the files, this helps!
impl Config for PolkaStorageConfig {
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

/// Currency as specified by the SCALE-encoded runtime.
type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
type BlockNumber = u32;

#[derive(Debug, Clone, serde::Deserialize, codec::Encode)]
pub struct ActiveDealState<BlockNumber> {
    pub sector_number: u64,
    pub sector_start_block: BlockNumber,
    pub last_updated_block: Option<BlockNumber>,
    pub slash_block: Option<BlockNumber>,
}

#[derive(Debug, Clone, serde::Deserialize, codec::Encode)]
pub enum DealState<BlockNumber> {
    Published,
    Active(ActiveDealState<BlockNumber>),
}

#[derive(Debug, Clone, serde::Deserialize, codec::Encode)]
pub struct DealProposal<Config, BlockNumber, Currency>
where
    Config: subxt::Config,
{
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub client: Config::Address,
    pub provider: Config::Address,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: DealState<BlockNumber>,
}

impl<Config, BlockNumber, Currency> DealProposal<Config, BlockNumber, Currency>
where
    Config: subxt::Config<Signature = MultiSignature>,
{
    fn sign<Keypair>(self, keypair: &Keypair) -> ClientDealProposal<Config, BlockNumber, Currency>
    where
        Keypair: Signer<Config>,
    {
        let encoded_deal_proposal = self.encode();

        let signature = keypair.sign(&encoded_deal_proposal);
        let client = MultiSignature::from(signature);

        ClientDealProposal {
            proposal: self,
            client,
        }
    }
}

pub struct ClientDealProposal<Config, BlockNumber, Currency>
where
    Config: subxt::Config,
{
    pub proposal: DealProposal<Config, BlockNumber, Currency>,
    pub client: Config::Signature, // OffchainSignature
}
