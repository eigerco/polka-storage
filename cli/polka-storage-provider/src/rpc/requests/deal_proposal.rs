use std::sync::Arc;

use storagext::{types::market::ClientDealProposal, MarketClientExt};

use crate::rpc::{
    requests::RpcRequest,
    server::{RpcServerState, ServerError},
    version::V0,
};

#[derive(Debug)]
pub struct RegisterDealProposalRequest(ClientDealProposal);

impl RpcRequest<V0> for RegisterDealProposalRequest {
    const NAME: &'static str = "deal_proposal";
    type Ok = cid::Cid;
    type Params = ClientDealProposal;

    fn params(&self) -> Self::Params {
        // This clone is kinda meh but it needs architecture level changes
        self.0.clone()
    }

    async fn handle(
        ctx: Arc<RpcServerState>,
        deal_proposal: Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        tracing::debug!(params = ?deal_proposal, "received request");

        // NOTE: not sure what to do with this result yet
        let _result = ctx
            .xt_client
            .publish_signed_storage_deals(&ctx.xt_keypair, vec![deal_proposal.clone()])
            .await?;

        // TODO(@jmg-duarte,#389,20/9/24): open the mechanism to receive this file
        // maybe put the CID in RocksDB and have an expiration mechanism attached to it
        // while it doesnt expire, it can receive the file

        return Ok(deal_proposal.deal_proposal.piece_cid);
    }
}
