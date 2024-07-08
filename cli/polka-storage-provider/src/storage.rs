use std::{io, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use axum::{
    body::Body,
    extract::{MatchedPath, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures::TryStreamExt;
use mater::{create_filestore, Cid, Config};
use tempfile::tempdir_in;
use tokio::{
    fs::{self, File},
    io::{AsyncRead, BufWriter},
    sync::broadcast::Receiver,
};
use tokio_util::io::{ReaderStream, StreamReader};
use tower_http::trace::TraceLayer;
use tracing::{error, info, info_span, instrument};
use uuid::Uuid;

use crate::cli::CliError;

/// Shared state of the storage server.
pub struct StorageServerState {
    pub storage_dir: PathBuf,
}

#[instrument(skip_all)]
pub async fn start_upload_server(
    state: Arc<StorageServerState>,
    listen_addr: SocketAddr,
    mut notify_shutdown_rx: Receiver<()>,
) -> Result<(), CliError> {
    // Configure router
    let router = configure_router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    // Start server
    info!("upload server started at: {listen_addr}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = notify_shutdown_rx.recv().await;
        })
        .await?;

    Ok(())
}

// TODO(no-ref,@cernicc,28/06/2024): Nicer error handling in handlers
fn configure_router(state: Arc<StorageServerState>) -> Router {
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

/// Handler for the upload endpoint. It receives a stream of bytes, coverts them
/// to a CAR file and returns the CID of the CAR file to the user.
async fn upload(
    State(state): State<Arc<StorageServerState>>,
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
    State(state): State<Arc<StorageServerState>>,
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

/// Reads bytes from the source and writes them to a CAR file.
async fn stream_contents_to_car<R>(
    folder: &std::path::Path,
    source: R,
) -> Result<Cid, Box<dyn std::error::Error>>
where
    R: AsyncRead + Unpin,
{
    // Create a storage folder if it doesn't exist.
    if !folder.exists() {
        info!("creating storage folder: {}", folder.display());
        fs::create_dir_all(folder).await?;
    }

    // Temp file which will be used to store the CAR file content. The temp
    // director has a randomized name and is created in the same folder as the
    // finalized uploads are stored.
    let temp_dir = tempdir_in(folder)?;
    let temp_file_path = temp_dir.path().join("temp.car");

    // Stream the body from source to the temp file.
    let file = File::create(&temp_file_path).await?;
    let writer = BufWriter::new(file);
    let cid = create_filestore(source, writer, Config::default()).await?;

    // If the file is successfully written, we can now move it to the final
    // location.
    let (_, final_content_path) = content_path(folder, cid);
    fs::rename(temp_file_path, &final_content_path).await?;
    info!(location = %final_content_path.display(), "CAR file created");

    Ok(cid)
}

/// Returns the tuple of file name and path for a specified Cid.
fn content_path(folder: &std::path::Path, cid: Cid) -> (String, PathBuf) {
    let name = format!("{cid}.car");
    let path = folder.join(&name);
    (name, path)
}
