use std::{fmt::Debug, future::Future, net::SocketAddr, sync::Arc};

use chrono::Utc;
use cli_primitives::Error;
use client::Request;
use error::ServerError;
use jsonrpsee::{
    core::ClientError,
    server::{Server, ServerHandle},
    RpcModule,
};
use methods::create_module;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::substrate;

mod client;
pub mod error;
pub mod methods;

pub use client::RpcClient;

/// Default address to bind the RPC server to.
pub const RPC_SERVER_DEFAULT_BIND_ADDR: &str = "127.0.0.1:8000";

/// A definition of an RPC method handler which can be registered with an [`RpcModule`].
pub trait RpcMethod {
    /// Method name.
    const NAME: &'static str;
    /// See [`ApiVersion`].
    const API_VERSION: ApiVersion;
    /// Successful response type.
    type Ok: Debug + Serialize + DeserializeOwned;
    /// Parameters type.
    type Params: Debug + Serialize + DeserializeOwned;

    /// Logic for this method.
    fn handle(
        ctx: Arc<RpcServerState>,
        params: Self::Params,
    ) -> impl Future<Output = Result<Self::Ok, ServerError>> + Send;
}

pub trait RpcMethodExt: RpcMethod {
    /// Register this method with an [`RpcModule`].
    fn register_async(module: &mut RpcModule<RpcServerState>) -> &mut jsonrpsee::MethodCallback
    where
        Self::Ok: Clone + 'static,
    {
        module
            .register_async_method(Self::NAME, move |params, ctx| async move {
                // Try to deserialize the params
                let params = params.parse().map_err(|e| {
                    tracing::error!("Failed to parse params: {:?}", e);
                    ServerError::invalid_params("Failed to parse params", None)
                })?;

                // Handle the method
                let ok = Self::handle(ctx, params).await?;
                Result::<_, jsonrpsee::types::ErrorObjectOwned>::Ok(ok)
            })
            .expect("method should be valid") // This is safe because we know the method registered is valid.
    }

    /// Create a request for this method.
    ///
    /// Returns [`Err`] if any of the parameters fail to serialize.
    fn request(params: Self::Params) -> Result<Request<Self::Ok>, serde_json::Error> {
        let params = serde_json::to_value(params).expect("params should serialize");

        Ok(Request {
            method_name: Self::NAME,
            params,
            result_type: std::marker::PhantomData,
            api_version: Self::API_VERSION,
        })
    }

    /// Call the method with the provided client and params.
    async fn call(client: &RpcClient, params: Self::Params) -> Result<Self::Ok, ClientError> {
        let response = client.call(Self::request(params)?).await?;
        Ok(response)
    }
}

/// Blanket implementation for all types that implement [`RpcMethod`].
impl<T> RpcMethodExt for T where T: RpcMethod {}

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
) -> Result<ServerHandle, Error> {
    let server = Server::builder().build(listen_addr).await?;

    let module = create_module(state.clone());
    let server_handle = server.start(module);

    Ok(server_handle)
}
