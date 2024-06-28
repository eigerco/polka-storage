use std::{io, net::SocketAddr, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Path, Request, State},
    http::{Error, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    BoxError, Router,
};
use chrono::Utc;
use clap::Parser;
use futures::{Stream, TryStreamExt};
use mater::Blockstore;
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
async fn upload(
    State(state): State<Arc<RpcServerState>>,
    request: Request,
) -> Result<(), (StatusCode, String)> {
    dbg!("Uploading file");
    stream_to_file(request.into_body().into_data_stream())
        .await
        .unwrap();
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

// Save a `Stream` to a file
async fn stream_to_file<S, E>(stream: S) -> Result<(), (StatusCode, String)>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    // TODO: Check if file is already a car file
    let path = "something.car";

    async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Stream the body to the Blockstore
        let mut block_store = Blockstore::new();
        block_store.read(body_reader).await.unwrap();

        // Create the file. `File` implements `AsyncWrite`.
        let path = std::path::Path::new(UPLOADS_DIRECTORY).join(path);
        let mut file = BufWriter::new(File::create(path).await?);

        // Copy the body into the file.
        // Uncomment this for error.
        // block_store.write(&mut file).await.unwrap();

        Ok::<_, io::Error>(())
    }
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}
