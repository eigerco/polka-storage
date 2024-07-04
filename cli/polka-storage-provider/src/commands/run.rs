use std::{io, net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use clap::Parser;
use futures::TryStreamExt;
use mater::Cid;
use tokio::{fs::File, signal, sync::Notify};
use tokio_util::io::{ReaderStream, StreamReader};
use tracing::info;
use url::Url;

use crate::{
    cli::CliError,
    rpc::server::{start_rpc_server, RpcServerState, RPC_SERVER_DEFAULT_BIND_ADDR},
    storage::{content_path, stream_contents_to_car},
    substrate,
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
    pub async fn run(&self) -> Result<(), CliError> {
        let substrate_client = substrate::init_client(self.rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
        });

        // Notify setup for graceful shutdown
        let shutdown = Arc::new(Notify::new());

        // Listen for shutdown signal
        let shutdown_clone = shutdown.clone();
        tokio::spawn(async move {
            signal::ctrl_c().await.expect("failed to listen for event");
            shutdown_clone.notify_one();
        });

        // Start both servers
        tokio::select! {
            _ = start_rpc_server(state.clone(), self.listen_addr, shutdown.clone()) => {
                info!("RPC server stopped");
            }
            _ = start_upload_server(state.clone(), shutdown.clone()) => {
                info!("Upload server stopped");
            }
        }

        info!("Storage provider stopped");

        Ok(())
    }
}

async fn upload(
    State(_state): State<Arc<RpcServerState>>,
    request: Request,
) -> Result<String, (StatusCode, String)> {
    // Body stream and reader
    let body_data_stream = request.into_body().into_data_stream();
    let body_with_io_error =
        body_data_stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
    let body_reader = StreamReader::new(body_with_io_error);

    let cid = stream_contents_to_car(body_reader).await.unwrap();
    Ok(cid.to_string())
}

async fn download(
    State(_state): State<Arc<RpcServerState>>,
    Path(cid): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    // Path to a CAR file
    let Ok(cid) = Cid::from_str(&cid) else {
        return Err((StatusCode::BAD_REQUEST, "cid incorrect format".to_string()));
    };
    let path = content_path(cid);

    // Check if the file exists
    if !path.exists() {
        // Stream the file
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    // Open car file
    let Ok(file) = File::open(path).await else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to open file".to_string(),
        ));
    };

    // convert the `AsyncRead` into a `Stream`
    let stream = ReaderStream::new(file);
    // convert the `Stream` into the Body
    let body = Body::from_stream(stream);

    // TODO(no-ref,@cernicc,03/07/2024): What should be the response headers?
    Ok(body.into_response())
}

// TODO(no-ref,@cernicc,28/06/2024): Move routing and handlers somewhere else
// TODO(no-ref,@cernicc,28/06/2024): Nicer error handling in handlers
fn configure_router(state: Arc<RpcServerState>) -> Router {
    Router::new()
        .route("/upload", post(upload))
        .route("/download/:cid", get(download))
        .with_state(state)
}

async fn start_upload_server(state: Arc<RpcServerState>, shutdown: Arc<Notify>) {
    // Configure router
    let router = configure_router(state);

    // TODO(no-ref,@cernicc,04/07/2024): handle error if the address is already
    // in use. This should be done when both servers will listen on the same
    // address
    let address = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();

    // Start server
    info!("Upload server started at: {address}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.notified().await })
        .await
        .unwrap();
}
