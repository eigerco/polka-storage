use std::{io, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use axum::{
    body::Body,
    extract::{FromRequest, MatchedPath, Multipart, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Router,
};
use futures::{TryFutureExt, TryStreamExt};
use mater::Cid;
use polka_storage_provider_common::commp::{
    calculate_piece_commitment, CommPError, ZeroPaddingReader,
};
use primitives_commitment::piece::PaddedPieceSize;
use primitives_proofs::RegisteredPoStProof;
use tokio::{
    fs::{self, File},
    io::{AsyncRead, BufWriter},
};
use tokio_util::{
    io::{ReaderStream, StreamReader},
    sync::CancellationToken,
};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::db::DealDB;

/// Shared state of the storage server.
pub struct StorageServerState {
    pub car_piece_storage_dir: Arc<PathBuf>,

    pub deal_db: Arc<DealDB>,

    pub listen_address: SocketAddr,
    // I think this just needs the sector size actually
    pub post_proof: RegisteredPoStProof,
}

#[tracing::instrument(skip_all)]
pub async fn start_upload_server(
    state: Arc<StorageServerState>,
    token: CancellationToken,
) -> Result<(), std::io::Error> {
    // Create a storage folder if it doesn't exist.
    if !state.car_piece_storage_dir.exists() {
        tracing::info!(folder = ?state.car_piece_storage_dir, "creating storage folder");
        fs::create_dir_all(state.car_piece_storage_dir.as_ref()).await?;
    }

    tracing::info!("Starting HTTP storage server at: {}", state.listen_address);
    let listener = tokio::net::TcpListener::bind(state.listen_address).await?;

    // Configure router
    let router = configure_router(state);
    // Start server
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            token.cancelled_owned().await;
            tracing::trace!("shutdown received");
        })
        .await
}

fn configure_router(state: Arc<StorageServerState>) -> Router {
    Router::new()
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

                tracing::info_span!(
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
///
/// This method supports both `multipart/form-data` and direct uploads.
///
/// For example:
/// ```bash
/// # Multipart form uploads
/// curl -X PUT -F "upload=@<filename>" "http://localhost:8001/upload/<file_cid>"
/// # Direct uploads
/// curl --file-upload "http://localhost:8001/upload/<file_cid>"
/// ```
#[tracing::instrument(skip_all, fields(cid))]
async fn upload(
    ref s @ State(ref state): State<Arc<StorageServerState>>,
    Path(cid): Path<String>,
    request: Request,
) -> Result<String, (StatusCode, String)> {
    let deal_cid = cid::Cid::from_str(&cid).map_err(|err| {
        tracing::error!(cid, "failed to parse cid");
        (StatusCode::BAD_REQUEST, err.to_string())
    })?;

    let deal_db_conn = state.deal_db.clone();
    // If the deal hasn't been accepted, reject the upload
    let proposed_deal =
        // Move the fetch to the blocking pool since the RocksDB API is sync
        tokio::task::spawn_blocking(move || match deal_db_conn.get_proposed_deal(deal_cid) {
            Ok(Some(proposed_deal)) => Ok(proposed_deal),
            Ok(None) => {
                tracing::error!(cid = %deal_cid, "deal proposal was not found");
                Err((
                    StatusCode::NOT_FOUND,
                    format!("cid \"{}\" was not found", cid),
                ))
            }
            Err(err) => {
                tracing::error!(%err, "failed to fetch proposed deal");
                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }
        }).await.map_err(|err| {
            tracing::error!(%err, "failed to execute blocking task");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })??;

    // Branching needed here since the resulting `StreamReader`s don't have the same type
    let file_cid = if request.headers().contains_key("Content-Type") {
        // Handle multipart forms
        let mut multipart = Multipart::from_request(request, &s)
            .await
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        let Some(field) = multipart
            .next_field()
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
            .await?
        else {
            return Err((StatusCode::BAD_REQUEST, "empty request".to_string()));
        };

        let field_reader = StreamReader::new(field.map_err(std::io::Error::other));
        stream_contents_to_car(state.car_piece_storage_dir.clone().as_ref(), field_reader)
            .await
            .map_err(|err| {
                tracing::error!(%err, "failed to store file into CAR archive");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            })?
    } else {
        // Read the request body into a CAR archive
        let body_reader = StreamReader::new(
            request
                .into_body()
                .into_data_stream()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
        );
        stream_contents_to_car(state.car_piece_storage_dir.clone().as_ref(), body_reader)
            .await
            .map_err(|err| {
                tracing::error!(%err, "failed to store file into CAR archive");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            })?
    };
    tracing::debug!("generated cid: {file_cid}");

    // NOTE(@jmg-duarte,03/10/2024): Maybe we should just register the file in RocksDB and keep a
    // background process that vacuums the disk as necessary to simplify error handling here

    let (_, file_path) = content_path(&state.car_piece_storage_dir, file_cid);
    let file = File::open(&file_path).await.map_err(|err| {
        tracing::error!(%err, path = %file_path.display(), "failed to open file");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?;
    let file_size = file
        .metadata()
        .map_ok(|metadata| metadata.len())
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    // Check the piece size first since it's the cheap check
    let piece_size = PaddedPieceSize::from_arbitrary_size(file_size);
    if !(proposed_deal.piece_size == *piece_size) {
        tracing::trace!(
            expected = proposed_deal.piece_size,
            actual = *piece_size,
            "piece size does not match the proposal piece size"
        );

        // Not handling the error since there's little to be done here...
        let _ = tokio::fs::remove_file(&file_path).await.inspect_err(
            |err| tracing::error!(%err, path = %file_path.display(), "failed to delete file"),
        );

        return Err((
            StatusCode::BAD_REQUEST,
            "piece size does not match proposal".to_string(),
        ));
    }

    let piece_path = file_path.clone();
    // Calculate the piece commitment in the blocking thread pool since `calculate_piece_commitment`
    // is CPU intensive — i.e. blocking — potentially improvement is to move this completely out of
    // the tokio runtime into an OS thread
    let piece_commitment_cid = tokio::task::spawn_blocking(move || -> Result<_, CommPError> {
        // Yes, we're reloading the file, this requires the std version
        let file = std::fs::File::open(&piece_path)?;
        let file_size = file.metadata()?.len();
        let piece_size = PaddedPieceSize::from_arbitrary_size(file_size);
        let buffered = std::io::BufReader::new(file);
        let reader = ZeroPaddingReader::new(buffered, *piece_size);
        let piece_commitment = calculate_piece_commitment(reader, piece_size)?;
        let piece_commitment_cid = piece_commitment.cid();
        tracing::debug!(path = %piece_path.display(), commp = %piece_commitment_cid, "calculated piece commitment");
        Ok(piece_commitment_cid)
    })
    .await
    .map_err(|err| {
        tracing::error!(%err, "failed to execute blocking task");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?
    .map_err(|err| {
        tracing::error!(%err, path = %file_path.display(), "failed to calculate piece commitment");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?;

    if proposed_deal.piece_cid != piece_commitment_cid {
        if let Err(err) = tokio::fs::remove_file(&file_path).await {
            // We log instead of returning an error since:
            // * this is not critical for the user
            // * this is not something the user should even know about
            tracing::error!(%err, path = %file_path.display(), "failed to remove uploaded piece");
        }

        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "calculated piece cid does not match the proposed deal; expected: {}, received: {}",
                proposed_deal.piece_cid, piece_commitment_cid
            ),
        ));
    }

    tracing::trace!("renaming car file");
    // We need to rename the file since the original storage name is based on the whole deal proposal CID,
    // however, the piece is stored based on its piece_cid
    tokio::fs::rename(
        file_path,
        content_path(&state.car_piece_storage_dir, piece_commitment_cid).1,
    )
    .map_err(|err| {
        tracing::error!(%err, "failed to rename the CAR file");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })
    .await?;

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
        tracing::error!(?e, cid, "cid incorrect format");
        (StatusCode::BAD_REQUEST, "cid incorrect format".to_string())
    })?;

    let (file_name, path) = content_path(&state.car_piece_storage_dir, cid);
    tracing::info!(?path, "file requested");

    // Check if the file exists
    if !path.exists() {
        tracing::error!(?path, "file not found");
        return Err((StatusCode::NOT_FOUND, "file not found".to_string()));
    }

    // Open car file
    let file = File::open(&path).await.map_err(|e| {
        tracing::error!(?e, ?path, "failed to open file");
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

/// Reads bytes from the source and writes them to a CAR file.
async fn stream_contents_to_car<R>(
    folder: &std::path::Path,
    source: R,
) -> Result<Cid, Box<dyn std::error::Error>>
where
    R: AsyncRead + Unpin,
{
    // Temp file which will be used to store the CAR file content. The temp
    // director has a randomized name and is created in the same folder as the
    // finalized uploads are stored.
    let temp_dir = tempfile::tempdir_in(folder)?;
    let temp_file_path = temp_dir.path().join("temp.car");
    tracing::trace!("writing file to {}", temp_file_path.display());

    // Stream the body from source to the temp file.
    let file = File::create(&temp_file_path).await?;
    let writer = BufWriter::new(file);
    let cid = mater::create_filestore(source, writer, mater::Config::default()).await?;
    tracing::trace!("finished writing the CAR archive");

    // If the file is successfully written, we can now move it to the final
    // location.
    let (_, final_content_path) = content_path(folder, cid);
    fs::rename(temp_file_path, &final_content_path).await?;
    tracing::info!(?final_content_path, "CAR file created");

    Ok(cid)
}
