use std::{fmt::Debug, future::Future, sync::Arc};

use jsonrpsee::RpcModule;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, debug_span, error, Instrument};
use uuid::Uuid;

use super::server::{RpcServerState, ServerError};

pub mod common;
pub mod wallet;

/// A trait for defining a versioned RPC request.
pub trait RpcRequest<Version> {
    /// Method name.
    const NAME: &'static str;
    /// Successful response type.
    type Ok: Clone + Debug + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// Parameters type.
    type Params: Clone + Debug + Serialize + DeserializeOwned + Send + Sync;

    /// Get request parameters.
    fn get_params(&self) -> Self::Params;

    /// A definition of an RPC request handle which can be registered with an
    /// [`RpcModule`]. This specifies how to handle some specific RPC request.
    fn handle(
        ctx: Arc<RpcServerState>,
        params: Self::Params,
    ) -> impl Future<Output = Result<Self::Ok, ServerError>> + Send;
}

/// Register the [`RpcRequest`] handle with the [`RpcModule`].
pub fn register_async<Request, Version>(
    module: &mut RpcModule<RpcServerState>,
) -> &mut jsonrpsee::MethodCallback
where
    Request: RpcRequest<Version>,
{
    module
        .register_async_method(Request::NAME, move |params, ctx| async move {
            // Try to deserialize the params
            let span =
                debug_span!("method", id = %Uuid::new_v4(), method = Request::NAME, params = ?params);
            let params = params.parse().map_err(|err| {
                error!(parent: span.clone(), ?err, ?params, "failed to parse params");
                ServerError::invalid_params("Failed to parse params", None)
            })?;

            // Handle the method
            let result = Request::handle(ctx, params).instrument(span.clone()).await;

            match &result {
                Ok(ok) => {
                    debug!(parent: span, response = ?ok, "handled successfully");
                }
                Err(err) => {
                    error!(parent: span, err = ?err, "error ocurred while handling")
                }
            }

            Result::<_, jsonrpsee::types::ErrorObjectOwned>::Ok(result?)
        })
        .expect("method should be valid") // This is safe because we know the method registered is valid.
}
