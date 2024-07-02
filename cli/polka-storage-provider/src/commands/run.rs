use std::{io, net::SocketAddr, sync::Arc};

use axum::{
    debug_handler,
    extract::{Path, Request, State},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use clap::Parser;
use futures::TryStreamExt;
use mater::{create_filestore, Config};
use tokio::{fs::File, io::BufWriter};
use tokio_util::io::StreamReader;
use tracing::info;
use url::Url;

use crate::{
    cli::CliError,
    rpc::server::{start_rpc_server, RpcServerState, RPC_SERVER_DEFAULT_BIND_ADDR},
    substrate,
};

const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

/// Directory where uploaded files are stored.
const UPLOADS_DIRECTORY: &str = "uploads";

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

        // Start RPC server
        let handle = start_rpc_server(state.clone(), self.listen_addr).await?;
        info!("RPC server started at {}", self.listen_addr);

        // Upload endpoint
        let router = configure_router(state);
        // TODO(no-ref,@cernicc,28/06/2024): Listen on the same address that rpc listens on
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
            .await
            .unwrap();

        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();

        // Monitor shutdown
        shutdown_signal().await;

        // Stop the Server
        let _ = handle.stop();

        // Wait for the server to stop
        handle.stopped().await;
        info!("RPC server stopped");

        Ok(())
    }
}

// TODO(no-ref,@cernicc,28/06/2024): Handle shutdown better
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
}

// TODO(no-ref,@cernicc,28/06/2024): Move somewhere else
// TODO(no-ref,@cernicc,28/06/2024): Handle response
// TODO(no-ref,@cernicc,28/06/2024): Better error handling
#[debug_handler]
async fn upload(
    State(state): State<Arc<RpcServerState>>,
    request: Request,
) -> Result<(), (StatusCode, String)> {
    // Body stream and reader
    let body_data_stream = request.into_body().into_data_stream();
    let body_with_io_error =
        body_data_stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
    let body_reader = StreamReader::new(body_with_io_error);

    // Stream the body, convert it to car and write it to the file]
    // TODO: Remove spawn. Currently used only to check if the future is Send
    tokio::spawn(async move {
        // Destination file
        let path = "something.car";
        let path = std::path::Path::new(UPLOADS_DIRECTORY).join(path);
        let file = Box::new(BufWriter::new(File::create(path).await.unwrap()));

        create_filestore(body_reader, file, Config::default()).await;
    });

    Ok(())
}

async fn download(
    State(state): State<Arc<RpcServerState>>,
    Path(cid): Path<String>,
) -> Result<(), (StatusCode, String)> {
    Ok(())
}

// TODO(no-ref,@cernicc,28/06/2024): Move somewhere else
fn configure_router(state: Arc<RpcServerState>) -> Router {
    Router::new()
        .route("/upload", post(upload))
        .route("/download/:cid", get(download))
        .with_state(state)
}
