use std::{fmt::Debug, net::SocketAddr, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use jsonrpsee::{proc_macros::rpc as jsonrpsee_rpc, server::Server};
use serde::{Deserialize, Serialize};
use storagext::{
    types::market::{ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal},
    MarketClientExt,
};
use subxt::ext::sp_core::crypto::Ss58Codec;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument};

use crate::{db::DealDB, rpc::RpcError};

/// RPC server shared state.
pub struct RpcServerState {
    pub server_info: ServerInfo,
    pub deal_db: Arc<DealDB>,
    /// The file storage directory. Used to check if a given piece has been uploaded or not.
    pub storage_dir: Arc<PathBuf>,
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
    async fn info(&self) -> Result<ServerInfo, RpcError>;

    #[method(name = "propose_deal")]
    async fn propose_deal(&self, deal: SxtDealProposal) -> Result<cid::Cid, RpcError>;

    /// Publish a deal, the published deal ID will be returned.
    #[method(name = "publish_deal")]
    async fn publish_deal(&self, deal: SxtClientDealProposal) -> Result<u64, RpcError>;
}

#[async_trait::async_trait]
impl StorageProviderRpcServer for RpcServerState {
    async fn info(&self) -> Result<ServerInfo, RpcError> {
        Ok(self.server_info.clone())
    }

    async fn propose_deal(&self, deal: SxtDealProposal) -> Result<cid::Cid, RpcError> {
        // We currently accept all deals
        Ok(self
            .deal_db
            .add_accepted_proposed_deal(&deal)
            .map_err(|err| RpcError::internal_error(err, None))?)
    }

    async fn publish_deal(&self, deal: SxtClientDealProposal) -> Result<u64, RpcError> {
        let deal_proposal_cid = deal
            .deal_proposal
            .cid()
            .map_err(|err| RpcError::internal_error(err, None))?;

        // Check if this deal proposal has been accepted or not, error if not
        if self
            .deal_db
            .get_proposed_deal(deal_proposal_cid)
            .map_err(|err| RpcError::internal_error(err, None))?
            .is_none()
        {
            return Err(RpcError::internal_error(
                "proposal has not been accepted",
                None,
            ));
        }

        // Check if the respective piece has been uploaded, error if not
        let piece_cid = deal.deal_proposal.piece_cid;
        let piece_path = self.storage_dir.join(format!("{piece_cid}.car"));
        if !piece_path.exists() || !piece_path.is_file() {
            return Err(RpcError::internal_error(
                "piece has not been uploaded yet",
                None,
            ));
        }

        // TODO(@jmg-duarte,#428,04/10/2024):
        // There's a small bug here, currently, xt_client waits for a "full extrisic submission"
        // meaning that it will wait until the block where it is included in is finalized
        // however, due to https://github.com/paritytech/subxt/issues/1668 it may wrongly fail.
        // Fixing this requires the xt_client not wait for the finalization, it's not hard to do
        // it just requires some API design
        let result = self
            .xt_client
            .publish_signed_storage_deals(&self.xt_keypair, vec![deal])
            .await?;

        let published_deals = result
            .events
            .find::<storagext::runtime::market::events::DealPublished>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| RpcError::internal_error(err, None))?;

        // We currently just support a single deal and if there's no published deals,
        // and error MUST've happened
        debug_assert_eq!(published_deals.len(), 1);

        Ok(published_deals[0].deal_id)
    }
}

/// Start the RPC server.
#[instrument(skip_all)]
pub async fn start_rpc_server(
    state: RpcServerState,
    listen_addr: SocketAddr,
    token: CancellationToken,
) -> Result<(), std::io::Error> {
    let server = Server::builder().build(listen_addr).await?;
    let server_handle = server.start(state.into_rpc());

    info!("RPC server started at {}", listen_addr);

    token.cancelled_owned().await;
    tracing::trace!("shutdown signal received, stopping the RPC server");
    let _ = server_handle.stop();

    tracing::trace!("waiting for the RPC server to stop");
    // NOTE(@jmg-duarte,01/10/2024): adding a timeout will add a "dis" to this "graceful" shutdown
    server_handle.stopped().await;

    Ok(())
}
