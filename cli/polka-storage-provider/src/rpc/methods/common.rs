use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::RpcRequest;
use crate::rpc::{
    server::{RpcServerState, ServerError},
    version::V0,
};

/// This RPC method exposes the system information.
#[derive(Debug)]
pub struct InfoRequest;

impl RpcRequest<V0> for InfoRequest {
    const NAME: &'static str = "info";
    type Ok = InfoResult;
    type Params = ();

    fn params(&self) -> Self::Params {
        ()
    }

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
