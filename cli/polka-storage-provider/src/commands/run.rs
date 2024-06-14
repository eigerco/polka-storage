use std::{net::SocketAddr, sync::Arc};

use chrono::Utc;
use clap::Parser;
use cli_primitives::Error;
use tracing::info;
use url::Url;

use crate::{
    rpc::{start_rpc, RpcServerState},
    substrate,
};

const SERVER_DEFAULT_BIND_ADDR: &str = "127.0.0.1:8000";
const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

/// Command to start the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct RunCommand {
    /// RPC API endpoint used by the parachain node.
    #[arg(short = 'n', long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub node_rpc_address: Url,
    /// Address used for RPC. By default binds on localhost on port 8000.
    #[arg(short = 'a', long, default_value = SERVER_DEFAULT_BIND_ADDR)]
    pub listen_addr: SocketAddr,
}

impl RunCommand {
    pub async fn run(&self) -> Result<(), Error> {
        let substrate_client = substrate::init_client(self.node_rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
        });

        // Start RPC server
        let handle = start_rpc(state, self.listen_addr).await?;
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
