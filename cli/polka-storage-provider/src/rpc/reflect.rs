use std::sync::Arc;

use futures::Future;
use jsonrpsee::{
    types::{ErrorObjectOwned, Params},
    RpcModule,
};
use serde::{Deserialize, Serialize};

use super::RpcServerState;

/// Type to be used by [`RpcMethod::handle`].
pub type Ctx = Arc<RpcServerState>;

/// A definition of an RPC method handler which:
/// - can be [registered](RpcMethodExt::register) with an [`RpcModule`].
pub trait RpcMethod {
    /// Method name.
    const NAME: &'static str;
    /// See [`ApiVersion`].
    const API_VERSION: ApiVersion;
    /// Return value of this method.
    type Ok: Serialize;

    /// Logic for this method.
    fn handle(
        ctx: Ctx,
        params: Params,
    ) -> impl Future<Output = Result<Self::Ok, ErrorObjectOwned>> + Send;

    /// Register this method with an [`RpcModule`].
    fn register(
        module: &mut RpcModule<RpcServerState>,
    ) -> Result<&mut jsonrpsee::MethodCallback, jsonrpsee::core::RegisterMethodError>
    where
        Self::Ok: Clone + 'static,
    {
        module.register_async_method(Self::NAME, move |params, ctx| async move {
            let ok = Self::handle(ctx, params).await?;
            Result::<_, jsonrpsee::types::ErrorObjectOwned>::Ok(ok)
        })
    }
}

/// Available API versions.
///
/// These are significant because they are expressed in the URL path against
/// which RPC calls are made, e.g `rpc/v0` or `rpc/v1`.
///
/// This information is important when using [`crate::rpc::client`].
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ApiVersion {
    V0,
    V1,
}
