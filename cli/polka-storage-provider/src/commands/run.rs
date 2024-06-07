use std::{net::SocketAddr, sync::Arc};

use chrono::Utc;
use clap::Parser;
use cli_primitives::Result;
use url::Url;

use crate::{
    rpc::{start_rpc, RpcServerState},
    substrate,
};

const SERVER_DEFAULT_BIND_ADDR: &str = "127.0.0.1:8000";
const SUBSTRATE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

/// Command to start the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct RunCommand {
    /// RPC API endpoint of the parachain node
    #[arg(short = 'n', long, default_value = SUBSTRATE_DEFAULT_RPC_ADDR)]
    pub node_rpc_address: Url,
    /// Address on which the storage provider will listen to
    #[arg(short = 'a', long, default_value = SERVER_DEFAULT_BIND_ADDR)]
    pub listen_addr: SocketAddr,
}

impl RunCommand {
    pub async fn handle(&self) -> Result<()> {
        let substrate_client = substrate::init_client(self.node_rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
        });

        let handle = start_rpc(state, self.listen_addr).await?;
        let handle_clone = handle.clone();
        tokio::spawn(handle_clone.stopped());

        // Monitor shutdown
        tokio::signal::ctrl_c().await?;
        let _ = handle.stop();

        Ok(())
    }
}
