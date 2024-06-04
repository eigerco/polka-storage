use jsonrpsee::types::{ErrorObjectOwned, Params};
use subxt_signer::sr25519::dev;
use tracing::info;

use crate::{
    polkadot::get_balance,
    rpc::reflect::{ApiVersion, Ctx, RpcMethod},
};

pub struct WalletBalance;
impl RpcMethod for WalletBalance {
    const NAME: &'static str = "wallet_balance";
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Ok = String;

    async fn handle(ctx: Ctx, params: Params<'_>) -> Result<Self::Ok, ErrorObjectOwned> {
        let account = dev::alice().public_key().into();
        let balance = get_balance(&ctx.substrate_client, &account).await.unwrap();
        dbg!(balance);
        todo!()
    }
}
