//! Contains the types required to parse the CLI arguments.
//!
//! Separated from the library ones as these ones are specific while the library ones are generic
//! and ensuring the generic ones are parseable is problematic (to say the least).

use std::sync::Arc;

mod cmd;

/// Currency as specified by the SCALE-encoded runtime.
type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
type BlockNumber = u32;

/// CID wrapper to get deserialization.
#[derive(Debug, Clone)]
pub struct CidWrapper(Cid);

impl Deref for CidWrapper {
    type Target = Cid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Into<Cid> for CidWrapper {
    fn into(self) -> Cid {
        self.0
    }
}

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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActiveDealState {
    pub sector_number: u64,
    pub sector_start_block: BlockNumber,
    pub last_updated_block: Option<BlockNumber>,
    pub slash_block: Option<BlockNumber>,
}

impl Into<storagext::ActiveDealState<BlockNumber>> for ActiveDealState {
    fn into(self) -> storagext::ActiveDealState<BlockNumber> {
        storagext::ActiveDealState {
            sector_number: self.sector_number,
            sector_start_block: self.sector_start_block,
            last_updated_block: self.last_updated_block,
            slash_block: self.slash_block,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub enum DealState {
    Published,
    Active(ActiveDealState),
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DealProposal {
    pub piece_cid: CidWrapper,
    pub piece_size: u64,
    pub client: AccountId32,
    pub provider: AccountId32,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: DealState,
}

// impl DealProposal {
//     fn sign<Keypair, Config>(self, keypair: &Keypair) -> ClientDealProposal
//     where
//         Keypair: Signer<Config>,
//         Config: subxt::Config<Signature = MultiSignature>,
//     {
//         let encoded_deal_proposal = self.encode();

//         let signature = keypair.sign(&encoded_deal_proposal);
//         let client = MultiSignature::from(signature);

//         ClientDealProposal {
//             proposal: self,
//             client,
//         }
//     }
// }
