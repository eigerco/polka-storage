use std::str::FromStr;

use cid::Cid;
use subxt::utils::{AccountId32, MultiSignature};

pub mod market;
pub mod runtime;

/// Address as specified by the SCALE-encoded runtime.
type Address = AccountId32;

struct AccountId32Visitor;

impl<'v> serde::de::Visitor<'v> for AccountId32Visitor {
    type Value = AccountId32;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("expected an ss58 string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        AccountId32::from_str(v)
            .map_err(|err| serde::de::Error::invalid_value(serde::de::Unexpected::Str(v), &self))
    }
}

// struct MultiSignatureVisitor;

// impl<'v> serde::de::Visitor<'v> for MultiSignatureVisitor {
//     type Value = MultiSignature;

//     fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//         formatter.write_str("expected a valid hex-formatted key (sr25518, ed25519, ecdsa)")
//     }

//     fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
//     where
//         E: serde::de::Error,
//     {
//         let secret_key_bytes = hex::decode(v).map_err(|err| serde::de::Unexpected::Str(v), &self)?;
//         let multi_signature = if let Ok(keypair) = sr25519::Keypair::from_secret_key(secret_key_bytes) {
//             MultiSignature::Sr25519(keypair.sign(message))
//         }
//         else if let Ok(keypair) = ed25519::Keypair::from_secret_key(secret_key_bytes) {}
//         else if let Ok(keypair) = ecdsa::Keypair::from_secret_key(secret_key_bytes) {}

//         MultiSignature::from_str(v)
//             .map_err(|err| serde::de::Error::invalid_value(serde::de::Unexpected::Str(v), &self))
//     }
// }

/// Currency as specified by the SCALE-encoded runtime.
type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
type BlockNumber = u32;

#[derive(Debug, serde::Deserialize)]
struct ActiveDealState {
    pub sector_number: u64,
    pub sector_start_block: BlockNumber,
    pub last_updated_block: Option<BlockNumber>,
    pub slash_block: Option<BlockNumber>,
}

#[derive(Debug, serde::Deserialize)]
enum DealState {
    Published,
    Active(ActiveDealState),
}

#[derive(Debug, serde::Deserialize)]
struct DealProposal {
    pub piece_cid: Cid,
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

struct ClientDealProposal {
    pub proposal: DealProposal,
    pub client: MultiSignature, // OffchainSignature
}
