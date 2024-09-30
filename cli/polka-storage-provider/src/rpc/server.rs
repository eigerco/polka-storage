use std::{
    fmt::{Debug, Display},
    net::SocketAddr,
};

use chrono::{DateTime, Utc};
use jsonrpsee::{
    proc_macros::rpc as jsonrpsee_rpc,
    server::Server,
    types::{
        error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
        ErrorObjectOwned,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use storagext::{types::market::ClientDealProposal as SxtClientDealProposal, MarketClientExt};
use subxt::ext::sp_core::crypto::Ss58Codec;
use tokio::sync::oneshot::Receiver;
use tracing::{info, instrument};

use crate::CliError;

/// RPC server shared state.
pub struct RpcServerState {
    pub server_info: ServerInfo,
    pub xt_client: storagext::Client,
    pub xt_keypair: storagext::multipair::MultiPairSigner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// The server's start time.
    pub start_time: DateTime<Utc>,

    /// The server's account ID.
    #[serde(deserialize_with = "deserialize_address")]
    #[serde(serialize_with = "serialize_address")]
    pub address: <storagext::PolkaStorageConfig as subxt::Config>::AccountId,
}

impl ServerInfo {
    /// Construct a new [`ServerInfo`] instance, start time will be set to [`Utc::now`].
    pub fn new(address: <storagext::PolkaStorageConfig as subxt::Config>::AccountId) -> Self {
        Self {
            start_time: Utc::now(),
            address,
        }
    }
}

fn serialize_address<S>(
    address: &<storagext::PolkaStorageConfig as subxt::Config>::AccountId,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&address.to_ss58check())
}

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

#[jsonrpsee_rpc(server, client, namespace = "v0")]
pub trait StorageProviderRpc {
    /// Fetch server information.
    #[method(name = "info")]
    async fn info(&self) -> Result<ServerInfo, ServerError>;

    /// Publish a deal, the CID of the deal is returned if the deal is accepted.
    #[method(name = "publish_deal")]
    async fn publish_deal(&self, deal: SxtClientDealProposal) -> Result<cid::Cid, ServerError>;
}

#[async_trait::async_trait]
impl StorageProviderRpcServer for RpcServerState {
    async fn info(&self) -> Result<ServerInfo, ServerError> {
        Ok(self.server_info.clone())
    }

    async fn publish_deal(&self, deal: SxtClientDealProposal) -> Result<cid::Cid, ServerError> {
        let cid = deal.deal_proposal.piece_cid;
        let _result = self
            .xt_client
            .publish_signed_storage_deals(&self.xt_keypair, vec![deal])
            .await?;
        Ok(cid)
    }
}

/// Start the RPC server.
#[instrument(skip_all)]
pub async fn start_rpc_server(
    state: RpcServerState,
    listen_addr: SocketAddr,
    notify_shutdown_rx: Receiver<()>,
) -> Result<(), CliError> {
    let server = Server::builder().build(listen_addr).await?;

    let server_handle = server.start(state.into_rpc());
    info!("RPC server started at {}", listen_addr);

    // Wait for shutdown signal. No need to handle the error. We stop the server
    // in any case.
    let _ = notify_shutdown_rx.await;

    // Stop returns an error if the server has already been stopped.
    // PRE-COND: the server is only shutdown by receiving from `notify_shutdown_rx`
    let _ = server_handle.stop();

    // Wait for server to be stopped
    server_handle.stopped().await;

    Ok(())
}

/// Error type for RPC server errors.
#[derive(Debug)]
pub struct ServerError {
    inner: ErrorObjectOwned,
}

impl std::error::Error for ServerError {}

impl Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error: {}", &self.inner)
    }
}

impl From<ServerError> for ErrorObjectOwned {
    fn from(err: ServerError) -> Self {
        err.inner
    }
}

impl ServerError {
    pub fn new(code: i32, message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self {
            inner: ErrorObjectOwned::owned(code, message.to_string(), data.into()),
        }
    }

    /// Construct an error with [`jsonrpsee::types::error::INTERNAL_ERROR_CODE`].
    pub fn internal_error(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INTERNAL_ERROR_CODE, message, data)
    }

    /// Construct an error with [`jsonrpsee::types::error::INVALID_PARAMS_CODE`].
    pub fn invalid_params(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INVALID_PARAMS_CODE, message, data)
    }
}

impl From<subxt::Error> for ServerError {
    fn from(err: subxt::Error) -> Self {
        Self::internal_error(err, None)
    }
}
