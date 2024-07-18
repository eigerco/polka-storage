use cid::Cid;
use codec::Encode;
use frame_support::sp_runtime::{AccountId32, MultiSignature};
use subxt::tx::Signer;

pub mod market;
pub mod runtime;

/// Address as specified by the SCALE-encoded runtime.
type Address = AccountId32;

#[derive(Debug, Clone, codec::Encode)]
struct CidWrapper(Cid);

// The CID has some issues that require a workaround for strings.
// For more details, see: <https://github.com/multiformats/rust-cid/issues/162>
impl<'de> serde::de::Deserialize<'de> for CidWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self(
            Cid::try_from(s.as_str()).map_err(|e| serde::de::Error::custom(format!("{e:?}")))?,
        ))
    }
}

impl Into<Cid> for CidWrapper {
    fn into(self) -> Cid {
        self.0
    }
}

/// Currency as specified by the SCALE-encoded runtime.
type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
type BlockNumber = u32;

#[derive(Debug, Clone, serde::Deserialize, codec::Encode)]
pub struct ActiveDealState {
    pub sector_number: u64,
    pub sector_start_block: BlockNumber,
    pub last_updated_block: Option<BlockNumber>,
    pub slash_block: Option<BlockNumber>,
}

#[derive(Debug, Clone, serde::Deserialize, codec::Encode)]
pub enum DealState {
    Published,
    Active(ActiveDealState),
}

#[derive(Debug, Clone, serde::Deserialize, codec::Encode)]
pub struct DealProposal {
    pub piece_cid: CidWrapper,
    pub piece_size: u64,
    pub client: Address,
    pub provider: Address,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: DealState,
}

impl DealProposal {
    fn sign<Keypair, Config>(self, keypair: &Keypair) -> ClientDealProposal
    where
        Keypair: Signer<Config>,
        Config: subxt::Config<Signature = subxt::utils::MultiSignature>,
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

pub struct ClientDealProposal {
    pub proposal: DealProposal,
    pub client: MultiSignature, // OffchainSignature
}
