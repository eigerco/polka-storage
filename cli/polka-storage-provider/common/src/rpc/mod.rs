mod error;

use chrono::{DateTime, Utc};
use jsonrpsee::proc_macros::rpc;
use primitives_proofs::RegisteredPoStProof;
use serde::Deserialize;
use storagext::types::market::{
    ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal,
};
use subxt::ext::sp_core::crypto::Ss58Codec;

pub use crate::rpc::error::RpcError;

#[rpc(server, client, namespace = "v0")]
pub trait StorageProviderRpc {
    /// Fetch server information.
    #[method(name = "info")]
    async fn info(&self) -> Result<ServerInfo, RpcError>;

    /// Propose a deal, the CID of the deal will be returned,
    /// the CID is part of the path for file uploads.
    #[method(name = "propose_deal")]
    async fn propose_deal(&self, deal: SxtDealProposal) -> Result<cid::Cid, RpcError>;

    /// Publish a deal, the published deal ID will be returned.
    #[method(name = "publish_deal")]
    async fn publish_deal(&self, deal: SxtClientDealProposal) -> Result<u64, RpcError>;
}

/// Storage Provider server information, such as start time and on-chain address.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    /// The server's start time.
    pub start_time: DateTime<Utc>,

    /// The server's account ID.
    #[serde(deserialize_with = "deserialize_address")]
    #[serde(serialize_with = "serialize_address")]
    pub address: <storagext::PolkaStorageConfig as subxt::Config>::AccountId,

    /// The registered kind of proof.
    pub post_proof: RegisteredPoStProof,
}

impl ServerInfo {
    /// Construct a new [`ServerInfo`] instance, start time will be set to [`Utc::now`].
    pub fn new(
        address: <storagext::PolkaStorageConfig as subxt::Config>::AccountId,
        post_proof: RegisteredPoStProof,
    ) -> Self {
        Self {
            start_time: Utc::now(),
            address,
            post_proof,
        }
    }
}

/// Serialize a Polka Storage AccountId as a SS58 string.
fn serialize_address<S>(
    address: &<storagext::PolkaStorageConfig as subxt::Config>::AccountId,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&address.to_ss58check())
}

/// Deserialize a Polka Storage AccountId from a SS58 string.
fn deserialize_address<'de, D>(
    deserializer: D,
) -> Result<<storagext::PolkaStorageConfig as subxt::Config>::AccountId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    <storagext::PolkaStorageConfig as subxt::Config>::AccountId::from_ss58check(&s)
        .map_err(|err| serde::de::Error::custom(format!("invalid ss58 string: {}", err)))
}
