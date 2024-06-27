use std::{net::SocketAddr, sync::Arc};

use chrono::Utc;
use clap::Parser;
use tracing::info;
use url::Url;

use crate::{
    rpc::server::{start_rpc_server, RpcServerState, RPC_SERVER_DEFAULT_BIND_ADDR},
    substrate, Error,
};

const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

/// Command to start the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct RunCommand {
    /// RPC API endpoint used by the parachain node.
    #[arg(long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub rpc_address: Url,
    /// Address and port used for RPC server.
    #[arg(long, default_value = RPC_SERVER_DEFAULT_BIND_ADDR)]
    pub listen_addr: SocketAddr,
}

impl RunCommand {
    pub async fn run(&self) -> Result<(), Error> {
        let substrate_client = substrate::init_client(self.rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
        });

        // Start RPC server
        let handle = start_rpc_server(state, self.listen_addr).await?;
        info!("RPC server started at {}", self.listen_addr);

        // Monitor shutdown
        tokio::signal::ctrl_c().await?;

        // Stop the Server
        let _ = handle.stop();

        // Wait for the server to stop
        handle.stopped().await;
        info!("RPC server stopped");

        Ok(())
    }
}
