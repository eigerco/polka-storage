use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::rpc::{error::ServerError, ApiVersion, RpcMethod, RpcServerState};

/// This RPC method exposes the system information.
#[derive(Debug)]
pub struct Info;

impl RpcMethod for Info {
    const NAME: &'static str = "info";
    const API_VERSION: ApiVersion = ApiVersion::V0;
    type Ok = InfoResult;
    type Params = ();

    async fn handle(ctx: Arc<RpcServerState>, _: Self::Params) -> Result<Self::Ok, ServerError> {
        Ok(InfoResult {
            start_time: ctx.start_time,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResult {
    pub start_time: DateTime<Utc>,
}
