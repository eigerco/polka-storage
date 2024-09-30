use std::{io, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use axum::{
    body::Body,
    extract::{MatchedPath, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Router,
};
use futures::{TryFutureExt, TryStreamExt};
use mater::Cid;
use tokio::{
    fs::{self, File},
    io::BufWriter,
};
use tokio_util::{
    io::{ReaderStream, StreamReader},
    sync::CancellationToken,
};
use tower_http::trace::TraceLayer;
use tracing::{error, info, info_span, instrument};
use uuid::Uuid;

use crate::db::DealDB;

/// Shared state of the storage server.
pub struct StorageServerState {
    pub storage_dir: Arc<PathBuf>,
    pub deal_db: Arc<DealDB>,
}

#[instrument(skip_all)]
pub async fn start_upload_server(
    state: Arc<StorageServerState>,
    listen_addr: SocketAddr,
    token: CancellationToken,
) -> Result<(), std::io::Error> {
    // Create a storage folder if it doesn't exist.
    if !state.storage_dir.exists() {
        info!(folder = ?state.storage_dir, "creating storage folder");
        fs::create_dir_all(state.storage_dir.as_ref()).await?;
    }

    // Configure router
    let router = configure_router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    // Start server
    info!("upload server started at: {listen_addr}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            token.cancelled_owned().await;
            tracing::trace!("shutdown received");
        })
        .await
}

fn configure_router(state: Arc<StorageServerState>) -> Router {
    Router::new()
        // NOTE(@jmg-duarte,02/10/2024): not only I am trusting the absolute GOAT (Daniel Stenberg)
        // https://curl.se/docs/httpscripting.html#put
        // This also worked "first try" while multi-part did not work at all!
        .route("/upload/:cid", put(upload))
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
    Path(cid): Path<String>,
    request: Request,
) -> Result<String, (StatusCode, String)> {
    let deal_cid = match cid::Cid::from_str(&cid) {
        Ok(cid) => cid,
        Err(err) => return Err((StatusCode::BAD_REQUEST, err.to_string())),
    };

    // If the deal hasn't been accepted, reject the upload
    let proposed_deal = match state.deal_db.get_proposed_deal(deal_cid) {
        Ok(Some(proposed_deal)) => proposed_deal,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("cid \"{}\" was not found", cid),
            ));
        }
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    };

    let mut body_reader = StreamReader::new(
        request
            .into_body()
            .into_data_stream()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
    );

    // We first write the file to disk and then verify it,
    // it's wasteful but mater does not have a "passthrough" mode
    // i.e. reads and verifies the CAR as it goes, outputting the original bytes
    let piece_path = state.storage_dir.join(proposed_deal.piece_cid.to_string());

    // Opening the file a single time with OpenOptions::new().create(true).read(true).write(true)
    // does not work to write and read for some reason...
    // The tokio::io::copy doesn't close the handler, neither into_inner() or a &mut ref worked
    // So that's why there's a "double open"

    let read = File::create(&piece_path)
        .and_then(|file| async move {
            let mut writer = BufWriter::new(file);
            tracing::trace!("copying files");
            // tokio::io::copy flushes for us!
            tokio::io::copy(&mut body_reader, &mut writer).await
        })
        .await
        .map_err(|err| {
            tracing::error!(%err, "failed to open the file");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })?;

    if read != proposed_deal.piece_size {
        return Err((
            StatusCode::BAD_REQUEST,
            "piece size does not match proposed deal".to_string(),
        ));
    }

    let piece = File::open(&piece_path).await.map_err(|err| {
        tracing::error!(%err, "failed to open the file");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?;

    // Check that the file has the right CID
    tracing::trace!("verifying cid");
    if let Err(err) = mater::verify_cid(piece, proposed_deal.piece_cid).await {
        tracing::error!(%err, deal_cid=%proposed_deal.piece_cid, "piece verification failed");

        // Try to remove the file since if the CID does not match there's no point in keeping it around
        if let Err(err) = tokio::fs::remove_file(&piece_path).await {
            // We log instead of returning an error since:
            // * this is not critical for the user
            // * this is not something the user should even know about
            tracing::error!(path = %piece_path.display(), %err, "failed to remove uploaded file");
        }

        if let mater::Error::IoError(err) = err {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                // Q: should we show the CIDs?
                "uploaded car file does not match the deal's piece cid".to_string(),
            ));
        }
    }

    Ok(proposed_deal.piece_cid.to_string())
}

/// Handler for the download endpoint. It receives a CID and streams the CAR
/// file back to the user.
async fn download(
    State(state): State<Arc<StorageServerState>>,
    Path(cid): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    // Path to a CAR file
    let cid = Cid::from_str(&cid).map_err(|e| {
        error!(?e, cid, "cid incorrect format");
        (StatusCode::BAD_REQUEST, "cid incorrect format".to_string())
    })?;

    let (file_name, path) = content_path(&state.storage_dir, cid);
    info!(?path, "file requested");

    // Check if the file exists
    if !path.exists() {
        error!(?path, "file not found");
        return Err((StatusCode::NOT_FOUND, "file not found".to_string()));
    }

    // Open car file
    let file = File::open(&path).await.map_err(|e| {
        error!(?e, ?path, "failed to open file");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to open file".to_string(),
        )
    })?;

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

/// Returns the tuple of file name and path for a specified Cid.
fn content_path(folder: &std::path::Path, cid: Cid) -> (String, PathBuf) {
    let name = format!("{cid}.car");
    let path = folder.join(&name);
    (name, path)
}
