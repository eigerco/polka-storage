use std::{net::SocketAddr, sync::Arc};

use chrono::Utc;
use jsonrpsee::server::{Server, ServerHandle};
use methods::create_module;
use tracing::info;

pub mod methods;
mod reflect;

pub struct RpcServerState {
    pub start_time: chrono::DateTime<Utc>,
}

pub async fn start_rpc(
    state: Arc<RpcServerState>,
    listen_addr: SocketAddr,
) -> cli_primitives::Result<ServerHandle> {
    let server = Server::builder().build(listen_addr).await?;

    let module = create_module(state.clone());
    let server_handle = server.start(module);

    info!("RPC server started at {}", listen_addr);

    Ok(server_handle)
}
