mod error;

use std::fmt;

use chrono::{DateTime, Utc};
use jsonrpsee::proc_macros::rpc;
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use serde::{Deserialize, Serialize};
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
    async fn propose_deal(&self, deal: SxtDealProposal) -> Result<CidString, RpcError>;

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

    pub seal_proof: RegisteredSealProof,

    pub post_proof: RegisteredPoStProof,
    pub proving_period_start: u64,
}

impl ServerInfo {
    /// Construct a new [`ServerInfo`] instance, start time will be set to [`Utc::now`].
    pub fn new(
        address: <storagext::PolkaStorageConfig as subxt::Config>::AccountId,
        seal_proof: RegisteredSealProof,
        post_proof: RegisteredPoStProof,
        proving_period_start: u64,
    ) -> Self {
        Self {
            start_time: Utc::now(),
            address,
            seal_proof,
            post_proof,
            proving_period_start,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CidString(String);

impl From<cid::Cid> for CidString {
    fn from(cid: cid::Cid) -> Self {
        CidString(cid.to_string())
    }
}

impl TryFrom<String> for CidString {
    type Error = cid::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        // Validate the string is a valid CID
        let _: cid::Cid = s.parse()?;
        Ok(CidString(s))
    }
}

impl fmt::Display for CidString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for CidString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
