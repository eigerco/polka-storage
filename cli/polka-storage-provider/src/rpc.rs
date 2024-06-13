use std::{future::Future, net::SocketAddr, sync::Arc};

use chrono::Utc;
use error::ServerError;
use jsonrpsee::{
    server::{Server, ServerHandle},
    types::Params,
    RpcModule,
};
use methods::create_module;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::substrate;

pub mod error;
pub mod methods;

/// A definition of an RPC method handler which can be registered with an [`RpcModule`].
pub trait RpcMethod {
    /// Method name.
    const NAME: &'static str;
    /// See [`ApiVersion`].
    const API_VERSION: ApiVersion;
    /// Successful response type.
    type Ok: Serialize;

    /// Logic for this method.
    fn handle(
        ctx: Arc<RpcServerState>,
        params: Params,
    ) -> impl Future<Output = Result<Self::Ok, ServerError>> + Send;

    /// Register this method with an [`RpcModule`].
    fn register_async(module: &mut RpcModule<RpcServerState>) -> &mut jsonrpsee::MethodCallback
    where
        Self::Ok: Clone + 'static,
    {
        module
            .register_async_method(Self::NAME, move |params, ctx| async move {
                let ok = Self::handle(ctx, params).await?;
                Result::<_, jsonrpsee::types::ErrorObjectOwned>::Ok(ok)
            })
            .expect("method should be valid") // This is safe because we know the method registered is valid.
    }
}

/// Available API versions.
///
/// These are significant because they are expressed in the URL path against
/// which RPC calls are made, e.g `rpc/v0` or `rpc/v1`.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ApiVersion {
    V0,
}

pub struct RpcServerState {
    pub start_time: chrono::DateTime<Utc>,
    pub substrate_client: substrate::Client,
}

pub async fn start_rpc(
    state: Arc<RpcServerState>,
    listen_addr: SocketAddr,
) -> cli_primitives::Result<ServerHandle> {
    let server = Server::builder().build(listen_addr).await?;

    let module = create_module(state.clone());
    let server_handle = server.start(module);

    Ok(server_handle)
}
