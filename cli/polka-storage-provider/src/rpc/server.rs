use std::{
    fmt::{Debug, Display},
    net::SocketAddr,
    sync::Arc,
};

use chrono::Utc;
use jsonrpsee::{
    server::Server,
    types::{
        error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
        ErrorObjectOwned,
    },
    RpcModule,
};
use serde_json::Value;
use tokio::sync::oneshot::Receiver;
use tracing::{info, instrument};

use super::{
    methods::{common::InfoRequest, register_async, wallet::WalletRequest},
    version::V0,
};
use crate::{substrate, CliError};

/// Default address to bind the RPC server to.
pub const RPC_SERVER_DEFAULT_BIND_ADDR: &str = "127.0.0.1:8000";

/// RPC server shared state.
pub struct RpcServerState {
    pub start_time: chrono::DateTime<Utc>,
    pub substrate_client: substrate::Client,
}

/// Start the RPC server.
#[instrument(skip_all)]
pub async fn start_rpc_server(
    state: Arc<RpcServerState>,
    listen_addr: SocketAddr,
    notify_shutdown_rx: Receiver<()>,
) -> Result<(), CliError> {
    let server = Server::builder().build(listen_addr).await?;

    let module = create_module(state);
    let server_handle = server.start(module);
    info!("RPC server started at {}", listen_addr);

    // Wait for shutdown signal. No need to handle the error. We stop the server
    // in any case.
    let _ = notify_shutdown_rx.await;

    // Stop returns an error if the server has already been stopped.
    // PRE-COND: the server is only shutdown by receiving from `notify_shutdown_rx`
    let _ = server_handle.stop();

    // Wait for server to be stopped
    server_handle.stopped().await;

    Ok(())
}

/// Initialize [`RpcModule`] and register the handlers
/// [`super::methods::RpcRequest::handle`] which are specifying how requests
/// should be processed.
pub fn create_module(state: Arc<RpcServerState>) -> RpcModule<RpcServerState> {
    let mut module = RpcModule::from_arc(state);

    register_async::<InfoRequest, V0>(&mut module);
    register_async::<WalletRequest, V0>(&mut module);

    module
}

/// Error type for RPC server errors.
#[derive(Debug)]
pub struct ServerError {
    inner: ErrorObjectOwned,
}

impl std::error::Error for ServerError {}

impl Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error: {}", &self.inner)
    }
}

impl From<ServerError> for ErrorObjectOwned {
    fn from(err: ServerError) -> Self {
        err.inner
    }
}

impl ServerError {
    pub fn new(code: i32, message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self {
            inner: ErrorObjectOwned::owned(code, message.to_string(), data.into()),
        }
    }

    /// Construct an error with [`jsonrpsee::types::error::INTERNAL_ERROR_CODE`].
    pub fn internal_error(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INTERNAL_ERROR_CODE, message, data)
    }

    /// Construct an error with [`jsonrpsee::types::error::INVALID_PARAMS_CODE`].
    pub fn invalid_params(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INVALID_PARAMS_CODE, message, data)
    }
}

impl From<subxt::Error> for ServerError {
    fn from(err: subxt::Error) -> Self {
        Self::internal_error(err, None)
    }
}
