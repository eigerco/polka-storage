use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

use chrono::Utc;
use clap::Parser;
use tokio::{
    signal,
    sync::broadcast::{self, Sender},
};
use tracing::info;
use url::Url;

use crate::{
    cli::CliError,
    rpc::server::{start_rpc_server, RpcServerState, RPC_SERVER_DEFAULT_BIND_ADDR},
    storage::start_upload_server,
    substrate,
};

/// Default RPC API endpoint used by the parachain node.
const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

/// Default storage path.
fn default_storage_path() -> PathBuf {
    let mut current_dir = env::current_dir().expect("failed to get current directory");
    current_dir.push("uploads");
    current_dir
}

/// Command to start the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct RunCommand {
    /// RPC API endpoint used by the parachain node.
    #[arg(long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub rpc_address: Url,
    /// Address and port used for RPC server.
    #[arg(long, default_value = RPC_SERVER_DEFAULT_BIND_ADDR)]
    pub listen_addr: SocketAddr,
    /// Directory where uploaded files are stored.
    #[arg(long, default_value = default_storage_path().into_os_string())]
    pub storage_dir: PathBuf,
}

impl RunCommand {
    pub async fn run(&self) -> Result<(), CliError> {
        let substrate_client = substrate::init_client(self.rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
            storage_dir: self.storage_dir.clone(),
        });

        // Setup shutdown channel
        let (notify_shutdown_tx, _) = broadcast::channel(1);

        // Start the tasks in the background
        let rpc_handler = tokio::spawn(start_rpc_server(
            state.clone(),
            self.listen_addr,
            notify_shutdown_tx.subscribe(),
        ));
        let upload_handler = tokio::spawn(start_upload_server(
            state.clone(),
            notify_shutdown_tx.subscribe(),
        ));

        // Wait for SIGTERM on the main thread and once received "unblock"
        signal::ctrl_c().await.expect("failed to listen for event");
        // Send the shutdown signal
        let _ = notify_shutdown_tx.send(());

        // We can't wait forever, but we wait on this first so we can give extra
        // time for any pending uploads to finish
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), rpc_handler).await;
        // And still limit the uploads to a bound anyways
        let _ = tokio::time::timeout(std::time::Duration::from_secs(30), upload_handler).await;

        info!("storage provider stopped");
        Ok(())
    }
}
