use chrono::{DateTime, Utc};
use jsonrpsee::types::Params;
use serde::{Deserialize, Serialize};

use crate::rpc::{error::ServerError, ApiVersion, Ctx, RpcMethod};

#[derive(Debug)]
pub struct Info;

impl RpcMethod for Info {
    const NAME: &'static str = "info";

    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Ok = InfoResult;

    async fn handle(ctx: Ctx, _params: Params<'_>) -> Result<Self::Ok, ServerError> {
        Ok(InfoResult {
            start_time: ctx.start_time,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResult {
    pub start_time: DateTime<Utc>,
}
