use chrono::{DateTime, Utc};
use jsonrpsee::types::{ErrorObjectOwned, Params};

use crate::rpc::reflect::{ApiVersion, Ctx, RpcMethod};

pub struct Info;

impl RpcMethod for Info {
    const NAME: &'static str = "info";

    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Ok = DateTime<Utc>;

    async fn handle(ctx: Ctx, _params: Params<'_>) -> Result<Self::Ok, ErrorObjectOwned> {
        Ok(ctx.start_time)
    }
}
