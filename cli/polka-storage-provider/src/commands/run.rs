use std::{net::SocketAddr, sync::Arc};

use chrono::Utc;
use clap::Parser;
use tokio::{signal, sync::oneshot};
use tracing::info;
use url::Url;

use crate::{
    cli::CliError,
    rpc::server::{start_rpc_server, RpcServerState, RPC_SERVER_DEFAULT_BIND_ADDR},
    substrate,
};

/// Default RPC API endpoint used by the parachain node.
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
    pub async fn run(&self) -> Result<(), CliError> {
        let substrate_client = substrate::init_client(self.rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
        });

        // Setup shutdown channel
        let (notify_shutdown_tx, notify_shutdown_rx) = oneshot::channel();

        // Start the server in the background
        let rpc_handler = tokio::spawn(start_rpc_server(
            state.clone(),
            self.listen_addr,
            notify_shutdown_rx,
        ));

        // Wait for SIGTERM on the main thread and once received "unblock"
        signal::ctrl_c().await.expect("failed to listen for event");
        // Send the shutdown signal
        let _ = notify_shutdown_tx.send(());

        // Give server some time to finish
        info!("shutting down server, killing it in 10sec");
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), rpc_handler).await;

        info!("storage provider stopped");
        Ok(())
    }
}
