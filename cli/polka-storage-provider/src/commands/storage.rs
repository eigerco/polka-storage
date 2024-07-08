use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

use clap::Parser;
use tokio::{signal, sync::oneshot};
use tracing::info;

use crate::{
    cli::CliError,
    storage::{start_upload_server, StorageServerState},
};

/// Creates a path relative to the current directory in the format `./uploads`
fn default_storage_path() -> PathBuf {
    let mut current_dir = env::current_dir().expect("failed to get current directory");
    current_dir.push("uploads");
    current_dir
}

/// Default address to bind the storage server to.
pub const STORAGE_SERVER_DEFAULT_BIND_ADDR: &str = "127.0.0.1:9000";

/// Command to start the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct StorageCommand {
    /// Address and port used for storage server.
    #[arg(long, default_value = STORAGE_SERVER_DEFAULT_BIND_ADDR)]
    pub listen_addr: SocketAddr,
    /// Directory where uploaded files are stored.
    #[arg(long, default_value = default_storage_path().into_os_string())]
    pub storage_dir: PathBuf,
}

impl StorageCommand {
    pub async fn run(&self) -> Result<(), CliError> {
        let state = Arc::new(StorageServerState {
            storage_dir: self.storage_dir.clone(),
        });

        // Setup shutdown channel
        let (notify_shutdown_tx, notify_shutdown_rx) = oneshot::channel();

        // Start the server in the background
        let upload_handler = tokio::spawn(start_upload_server(
            state.clone(),
            self.listen_addr,
            notify_shutdown_rx,
        ));

        // Wait for SIGTERM on the main thread and once received "unblock"
        signal::ctrl_c().await.expect("failed to listen for event");
        // Send the shutdown signal
        let _ = notify_shutdown_tx.send(());

        // Give uploads some time to finish
        info!("shutting down server, killing it in 30sec");
        let _ = tokio::time::timeout(std::time::Duration::from_secs(30), upload_handler).await;

        info!("storage provider server stopped");
        Ok(())
    }
}
