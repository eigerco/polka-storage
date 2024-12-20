use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::http::Method;
use jsonrpsee::server::Server;
use polka_storage_provider_common::rpc::{
    CidString, RpcError, ServerInfo, StorageProviderRpcServer,
};
use primitives::commitment::{CommP, Commitment, CommitmentKind};
use storagext::{
    types::market::{ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal},
    MarketClientExt,
};
use subxt::tx::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, instrument};

use crate::{
    db::DealDB,
    pipeline::types::{AddPieceMessage, PipelineMessage},
};

/// RPC server shared state.
pub struct RpcServerState {
    pub server_info: ServerInfo,
    pub deal_db: Arc<DealDB>,

    /// The file storage directory. Used to check if a given piece has been uploaded or not.
    pub car_piece_storage_dir: Arc<PathBuf>,

    pub xt_client: Arc<storagext::Client>,
    pub xt_keypair: storagext::multipair::MultiPairSigner,

    pub listen_address: SocketAddr,
    pub pipeline_sender: UnboundedSender<PipelineMessage>,
}

#[async_trait::async_trait]
impl StorageProviderRpcServer for RpcServerState {
    async fn info(&self) -> Result<ServerInfo, RpcError> {
        Ok(self.server_info.clone())
    }

    async fn propose_deal(&self, deal: SxtDealProposal) -> Result<CidString, RpcError> {
        // TODO(@jmg-duarte,26/11/2024): proper unit or e2e testing of these validations

        if deal.piece_size > self.server_info.post_proof.sector_size().bytes() {
            return Err(RpcError::invalid_params(
                "Piece size cannot be larger than the registered sector size",
                None,
            ));
        }

        if deal.start_block > deal.end_block {
            return Err(RpcError::invalid_params(
                "start_block cannot be after end_block",
                None,
            ));
        }

        if deal.provider != self.xt_keypair.account_id() {
            return Err(RpcError::invalid_params(
                "deal's provider ID does not match the current provider ID",
                None,
            ));
        }

        if deal.piece_cid.codec() != CommP::multicodec() {
            return Err(RpcError::invalid_params(
                "piece_cid is not a piece commitment",
                None,
            ));
        }

        if !deal.piece_size.is_power_of_two() {
            return Err(RpcError::invalid_params(
                "invalid piece_size, must be a power of two",
                None,
            ));
        }

        if deal.storage_price_per_block == 0 {
            return Err(RpcError::invalid_params(
                "storage_price_per_block must be greater than 0",
                None,
            ));
        }

        let storage_provider_balance = self
            .xt_client
            .retrieve_balance(self.xt_keypair.account_id())
            .await?
            .ok_or_else(|| RpcError::internal_error("Storage Provider not found", None))?;

        if storage_provider_balance.free < deal.provider_collateral {
            return Err(RpcError::invalid_params(
                "storage provider balance is lower than the deal's collateral",
                None,
            ));
        }

        let client_balance = self
            .xt_client
            .retrieve_balance(deal.client.clone())
            .await?
            .ok_or_else(|| RpcError::internal_error("Client not found", None))?;

        if client_balance.free < deal.cost() {
            return Err(RpcError::invalid_params(
                "client's balance is lower than the deal's cost",
                None,
            ));
        }

        let cid = self
            .deal_db
            .add_accepted_proposed_deal(&deal)
            .map_err(|err| RpcError::internal_error(err, None))?;

        Ok(CidString::from(cid))
    }

    async fn publish_deal(&self, deal: SxtClientDealProposal) -> Result<u64, RpcError> {
        if deal.deal_proposal.piece_size > self.server_info.post_proof.sector_size().bytes() {
            // once again, the rpc error is wrong, we'll need to fix that
            return Err(RpcError::invalid_params(
                "Piece size cannot be larger than the registered sector size",
                None,
            ));
        }

        let deal_proposal_cid = deal
            .deal_proposal
            .json_cid()
            .map_err(|err| RpcError::internal_error(err, None))?;

        // Check if this deal proposal has been accepted or not, error if not
        if self
            .deal_db
            .get_proposed_deal(deal_proposal_cid)
            .map_err(|err| RpcError::internal_error(err, None))?
            .is_none()
        {
            return Err(RpcError::internal_error(
                "proposal has not been found — have you proposed the deal first?",
                None,
            ));
        }

        // Check if the respective piece has been uploaded, error if not
        let piece_cid = deal.deal_proposal.piece_cid;
        let piece_path = self.car_piece_storage_dir.join(format!("{piece_cid}.car"));
        if !piece_path.exists() || !piece_path.is_file() {
            return Err(RpcError::internal_error(
                "piece has not been uploaded yet",
                None,
            ));
        }

        // TODO(@jmg-duarte,25/11/2024): don't batch the deals for better errors

        let deal_proposal = deal.deal_proposal.clone();
        // TODO(@jmg-duarte,#428,04/10/2024):
        // There's a small bug here, currently, xt_client waits for a "full extrisic submission"
        // meaning that it will wait until the block where it is included in is finalized
        // however, due to https://github.com/paritytech/subxt/issues/1668 it may wrongly fail.
        // Fixing this requires the xt_client not wait for the finalization, it's not hard to do
        // it just requires some API design
        let result = self
            .xt_client
            .publish_signed_storage_deals(&self.xt_keypair, vec![deal], true)
            .await?
            .expect("we're waiting for the finalization so it should NEVER be None");

        let published_deals = result
            .events
            .find_first::<storagext::runtime::market::events::DealsPublished>()
            .map_err(|err| RpcError::internal_error(err, None))?;
        let Some(published_deals) = published_deals else {
            return Err(RpcError::internal_error(
                "failed to find any published deals",
                None,
            ));
        };

        // We currently just support a single deal and if there's no published deals,
        // an error MUST've happened
        debug_assert_eq!(published_deals.deals.0.len(), 1);

        // We always publish only 1 deal
        let deal_id = published_deals
            .deals
            .0
            .first()
            .expect("we only support a single deal")
            .deal_id;

        let commitment = Commitment::from_cid(&piece_cid).map_err(|e| {
            RpcError::invalid_params(
                e,
                Some(serde_json::to_value(piece_cid).expect("cid to be serializable")),
            )
        })?;

        self.pipeline_sender
            .send(PipelineMessage::AddPiece(AddPieceMessage {
                deal: deal_proposal,
                published_deal_id: deal_id,
                piece_path,
                commitment,
            }))
            .map_err(|e| RpcError::internal_error(e, None))?;

        Ok(deal_id)
    }
}

/// Start the RPC server.
#[instrument(skip_all)]
pub async fn start_rpc_server(
    state: RpcServerState,
    token: CancellationToken,
) -> Result<(), std::io::Error> {
    info!("Starting RPC server at {}", state.listen_address);

    let cors = CorsLayer::new()
        .allow_methods([Method::POST])
        .allow_origin(Any)
        .allow_headers([hyper::header::CONTENT_TYPE]);

    let middleware = tower::ServiceBuilder::new().layer(cors);

    let server = Server::builder()
        .set_http_middleware(middleware)
        .build(state.listen_address)
        .await?;

    let rpc = StorageProviderRpcServer::into_rpc(state);
    let server_handle = server.start(rpc);
    info!("RPC server started");

    token.cancelled_owned().await;
    tracing::trace!("shutdown signal received, stopping the RPC server");
    let _ = server_handle.stop();

    tracing::trace!("waiting for the RPC server to stop");
    // NOTE(@jmg-duarte,01/10/2024): adding a timeout will add a "dis" to this "graceful" shutdown
    server_handle.stopped().await;

    Ok(())
}
