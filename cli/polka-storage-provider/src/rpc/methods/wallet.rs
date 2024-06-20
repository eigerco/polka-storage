use std::sync::Arc;

use serde::{Deserialize, Serialize};
use subxt_signer::sr25519::dev;

use crate::{
    rpc::{error::ServerError, ApiVersion, RpcMethod, RpcServerState},
    substrate::get_system_balances,
};

/// This RPC method exposes getting the system balances for the particular
/// account.
pub struct WalletBalance;
impl RpcMethod for WalletBalance {
    const NAME: &'static str = "wallet_balance";
    const API_VERSION: ApiVersion = ApiVersion::V0;
    type Ok = Option<WalletBalanceResult>;
    type Params = ();

    async fn handle(ctx: Arc<RpcServerState>, _: Self::Params) -> Result<Self::Ok, ServerError> {
        // TODO(#68,@cernicc,05/06/2024): Implement correctly. dev alice is used as a show case for now.
        let account = dev::alice().public_key().into();
        let balance = get_system_balances(&ctx.substrate_client, &account).await?;

        Ok(balance.map(|balance| WalletBalanceResult {
            free: balance.data.free.to_string(),
            reserved: balance.data.reserved.to_string(),
            frozen: balance.data.frozen.to_string(),
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceResult {
    free: String,
    reserved: String,
    frozen: String,
}