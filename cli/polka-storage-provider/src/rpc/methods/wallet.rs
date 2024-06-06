use jsonrpsee::types::{ErrorObjectOwned, Params};
use serde::{Deserialize, Serialize};
use subxt_signer::sr25519::dev;

use crate::{
    polkadot::get_balance,
    rpc::{ApiVersion, Ctx, RpcMethod},
};

pub struct WalletBalance;
impl RpcMethod for WalletBalance {
    const NAME: &'static str = "wallet_balance";
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Ok = Option<WalletBalanceResult>;

    async fn handle(ctx: Ctx, _params: Params<'_>) -> Result<Self::Ok, ErrorObjectOwned> {
        // TODO(@cernicc,05/06/2024): Implement correctly. dev alice is used as a show case for now.
        let account = dev::alice().public_key().into();
        // TODO(@cernicc,05/06/2024): Handle error.
        let balance = get_balance(&ctx.substrate_client, &account).await.unwrap();

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
