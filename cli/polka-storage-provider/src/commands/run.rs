use std::{io, net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    body::Body,
    extract::{MatchedPath, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use clap::Parser;
use futures::TryStreamExt;
use mater::Cid;
use tokio::{
    fs::File,
    signal,
    sync::broadcast::{self, Receiver, Sender},
    try_join,
};
use tokio_util::io::{ReaderStream, StreamReader};
use tower_http::trace::TraceLayer;
use tracing::{error, info, info_span, instrument};
use url::Url;
use uuid::Uuid;

use crate::{
    cli::CliError,
    rpc::server::{start_rpc_server, RpcServerState, RPC_SERVER_DEFAULT_BIND_ADDR},
    storage::{content_path, stream_contents_to_car, STORAGE_DEFAULT_DIRECTORY},
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
    /// Directory where uploaded files are stored.
    #[arg(long, default_value = STORAGE_DEFAULT_DIRECTORY)]
    pub storage_dir: String,
}

impl RunCommand {
    pub async fn run(&self) -> Result<(), CliError> {
        let substrate_client = substrate::init_client(self.rpc_address.as_str()).await?;

        let state = Arc::new(RpcServerState {
            start_time: Utc::now(),
            substrate_client,
            storage_dir: self.storage_dir.clone(),
        });

        // Setup shutdown mechanism
        let (notify_shutdown_tx, _) = broadcast::channel(1);
        tokio::spawn(shutdown_trigger(notify_shutdown_tx.clone()));

        // Start both servers
        try_join!(
            start_rpc_server(
                state.clone(),
                self.listen_addr,
                notify_shutdown_tx.subscribe()
            ),
            start_upload_server(state.clone(), notify_shutdown_tx.subscribe())
        )?;

        info!("storage provider stopped");
        Ok(())
    }
}

async fn shutdown_trigger(notify_shutdown_tx: Sender<()>) {
    // Listen for the shutdown signal
    signal::ctrl_c().await.expect("failed to listen for event");

    // Notify the shutdown
    info!("shutdown signal received");
    let _ = notify_shutdown_tx.send(());
}

/// Handler for the upload endpoint. It receives a stream of bytes, coverts them
/// to a CAR file and returns the CID of the CAR file to the user.
async fn upload(
    State(state): State<Arc<RpcServerState>>,
    request: Request,
) -> Result<String, (StatusCode, String)> {
    // Body reader
    let body_reader = StreamReader::new(
        request
            .into_body()
            .into_data_stream()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
    );

    stream_contents_to_car(&state.storage_dir, body_reader)
        .await
        .map_err(|err| {
            error!(?err, "failed to create a CAR file");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create a CAR file".to_string(),
            )
        })
        .map(|cid| cid.to_string())
}

/// Handler for the download endpoint. It receives a CID and streams the CAR
/// file back to the user.
async fn download(
    State(state): State<Arc<RpcServerState>>,
    Path(cid): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    // Path to a CAR file
    let Ok(cid) = Cid::from_str(&cid) else {
        error!(cid, "cid incorrect format");
        return Err((StatusCode::BAD_REQUEST, "cid incorrect format".to_string()));
    };
    let (file_name, path) = content_path(&state.storage_dir, cid);
    info!(path = %path.display(), "file requested");

    // Check if the file exists
    if !path.exists() {
        error!(?path, "file not found");
        return Err((StatusCode::NOT_FOUND, "file not found".to_string()));
    }

    // Open car file
    let Ok(file) = File::open(path).await else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to open file".to_string(),
        ));
    };

    // Convert the `AsyncRead` into a `Stream`
    let stream = ReaderStream::new(file);
    // Convert the `Stream` into the Body
    let body = Body::from_stream(stream);
    // Response headers
    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream"),
        (
            header::CONTENT_DISPOSITION,
            &format!("attachment; filename=\"{:?}\"", file_name),
        ),
    ];

    Ok((headers, body).into_response())
}

// TODO(no-ref,@cernicc,28/06/2024): Move routing and handlers somewhere else
// TODO(no-ref,@cernicc,28/06/2024): Nicer error handling in handlers
fn configure_router(state: Arc<RpcServerState>) -> Router {
    Router::new()
        .route("/upload", post(upload))
        .route("/download/:cid", get(download))
        .with_state(state)
        // Tracing layer
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                // Log the matched route's path (with placeholders not filled in).
                // Use request.uri() or OriginalUri if you want the real path.
                let matched_path = request
                    .extensions()
                    .get::<MatchedPath>()
                    .map(MatchedPath::as_str);

                info_span!(
                    "request",
                    method = ?request.method(),
                    matched_path,
                    request_id = %Uuid::new_v4()
                )
            }),
        )
}

#[instrument(skip_all)]
async fn start_upload_server(
    state: Arc<RpcServerState>,
    mut notify_shutdown_rx: Receiver<()>,
) -> Result<(), CliError> {
    // Configure router
    let router = configure_router(state);

    // TODO(no-ref,@cernicc,04/07/2024): handle error if the address is already
    // in use. This should be done when both servers will listen on the same
    // address
    let address = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(address).await?;

    // Start server
    info!("upload server started at: {address}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = notify_shutdown_rx.recv().await;
        })
        .await?;

    Ok(())
}
