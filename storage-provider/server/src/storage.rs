use std::{io, net::SocketAddr, path::PathBuf, pin::Pin, str::FromStr, sync::Arc};

use axum::{
    body::Body,
    extract::{FromRequest, MatchedPath, Multipart, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Router,
};
use bytes::Bytes;
use futures::{Stream, TryStreamExt};
use mater::{create_filestore, Cid, Config};
use polka_storage_provider_common::commp::{commp, CommPError};
use primitives::{commitment::piece::PaddedPieceSize, proofs::RegisteredPoStProof};
use tokio::{
    fs::{self, File},
    io::{AsyncRead, BufWriter},
};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

// Add these missing imports:
use tokio_util::io::{ReaderStream, StreamReader};
use tokio_util::sync::CancellationToken;

/// A boxed stream of bytes for reading request content
type BoxedStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

#[cfg(feature = "delia")]
mod delia_imports {
    pub use axum::{http::Method, response::Json, routing::post};
    pub use codec::Encode;
    pub use storagext::{
        runtime::runtime_types::pallet_market::pallet::DealProposal as RuntimeDealProposal,
        types::market::DealProposal as SxtDealProposal,
    };
    pub use tower_http::cors::{Any, CorsLayer};
}

#[cfg(feature = "delia")]
use delia_imports::*;

use crate::db::DealDB;

/// Shared state of the storage server.
pub struct StorageServerState {
    pub car_piece_storage_dir: Arc<PathBuf>,
    pub deal_db: Arc<DealDB>,
    pub listen_address: SocketAddr,
    // I think this just needs the sector size actually
    #[allow(dead_code)]
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
    #[cfg(feature = "delia")]
    fn config_delia(state: Arc<StorageServerState>) -> Router {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE])
            .max_age(std::time::Duration::from_secs(3600));

        Router::new()
            .route("/upload/:cid", put(upload))
            .route("/download/:cid", get(download))
            .route("/calculate_piece_cid", put(calculate_piece_cid))
            .route("/encode_proposal", post(encode_proposal))
            .layer(cors)
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

    #[cfg(not(feature = "delia"))]
    fn config_non_delia(state: Arc<StorageServerState>) -> Router {
        // Type annotation required to satisfy Send bounds needed for UnixFS processing
        // across async operations and thread boundaries
        Router::new()
            .route(
                "/upload/:cid",
                put(upload as fn(State<Arc<StorageServerState>>, Path<String>, Request<Body>) -> _),
            )
            .route("/download/:cid", get(download))
            .with_state(state)
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

    #[cfg(feature = "delia")]
    let router = config_delia(state);
    #[cfg(not(feature = "delia"))]
    let router = config_non_delia(state);

    router
}

/// Handler for the upload endpoint. It receives a stream of bytes, converts them
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
    State(ref state): State<Arc<StorageServerState>>,
    Path(cid): Path<String>,
    request: Request,
) -> Result<String, (StatusCode, String)> {
    // Parse the provided CID.
    let deal_cid = cid::Cid::from_str(&cid).map_err(|err| {
        tracing::error!(cid, "failed to parse cid");
        (StatusCode::BAD_REQUEST, err.to_string())
    })?;

    // Use deal_db (we need it now, so we clone it)
    let deal_db_conn = state.deal_db.clone();
    // If the deal hasn't been accepted, reject the upload.
    let proposed_deal =
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
        })
        .await
        .map_err(|err| {
            tracing::error!(%err, "failed to execute blocking task");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })??;

    // Determine how to obtain the file's bytes:
    let file_cid = if request.headers().contains_key("Content-Type") {
        // For multipart/form-data, we stream the field contents.
        let state_clone = state.clone();
        let stream: BoxedStream = Box::pin(async_stream::try_stream! {
            let mut multipart = Multipart::from_request(request, &state_clone)
                .await
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
            // Get the next field.
            let mut field = multipart
                .next_field()
                .await
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "empty request"))?;
            // Yield each chunk as it becomes available.
            while let Ok(Some(chunk)) = field.chunk().await {
                yield chunk;
            }
        });
        let field_reader = StreamReader::new(stream);
        stream_contents_to_car(state.car_piece_storage_dir.as_ref(), field_reader)
            .await
            .map_err(|err| {
                tracing::error!(%err, "failed to store file into CAR archive");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            })?
    } else {
        // For direct uploads, convert the request body into a stream.
        let body_reader = StreamReader::new(
            request
                .into_body()
                .into_data_stream()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
        );
        stream_contents_to_car(state.car_piece_storage_dir.as_ref(), body_reader)
            .await
            .map_err(|err| {
                tracing::error!(%err, "failed to store file into CAR archive");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            })?
    };

    tracing::debug!("generated cid: {file_cid}");

    // Open the CAR file to check its size.
    let (_, file_path) = content_path(&state.car_piece_storage_dir, file_cid);
    let file = File::open(&file_path).await.map_err(|err| {
        tracing::error!(%err, path = %file_path.display(), "failed to open file");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?;
    let file_size = file
        .metadata()
        .await
        .map(|m| m.len())
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    // Check that the piece size matches the proposal.
    let piece_size = PaddedPieceSize::from_arbitrary_size(file_size);
    if proposed_deal.piece_size != *piece_size {
        tracing::trace!(
            expected = proposed_deal.piece_size,
            actual = *piece_size,
            "piece size does not match the proposal piece size"
        );
        let _ = tokio::fs::remove_file(&file_path).await;
        return Err((
            StatusCode::BAD_REQUEST,
            "piece size does not match proposal".to_string(),
        ));
    }

    let piece_path = file_path.clone();
    // Calculate the piece commitment in a blocking task.
    let piece_commitment_cid = tokio::task::spawn_blocking(move || -> Result<_, CommPError> {
        let (piece_commitment, _) = commp(&piece_path)?;
        let piece_commitment_cid = piece_commitment.cid();
        tracing::debug!(
            path = %piece_path.display(),
            commp = %piece_commitment_cid,
            "calculated piece commitment"
        );
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
        let _ = tokio::fs::remove_file(&file_path).await;
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "calculated piece cid does not match the proposed deal; expected: {}, received: {}",
                proposed_deal.piece_cid, piece_commitment_cid
            ),
        ));
    }

    tracing::trace!("renaming car file");
    tokio::fs::rename(
        file_path,
        content_path(&state.car_piece_storage_dir, piece_commitment_cid).1,
    )
    .await
    .map_err(|err| {
        tracing::error!(%err, "failed to rename the CAR file");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?;

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

/// Converts a source stream into a CARv2 file and writes it to an output stream.
///
/// Send + 'static bounds are required because the UnixFS processing involves:
/// - Async stream processing that may cross thread boundaries
/// - State management for DAG construction and deduplication
/// - Block tracking that must be thread-safe
///
/// The expanded trait bounds ensure that all data can be safely moved between
/// threads during async operations.
async fn stream_contents_to_car<R>(
    folder: &std::path::Path,
    source: R,
) -> Result<Cid, Box<dyn std::error::Error>>
where
    R: AsyncRead + Unpin + Send + 'static,
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

    let config = Config::default();

    let cid = create_filestore(source, writer, config).await?;
    tracing::trace!("finished writing the CAR archive");

    // If the file is successfully written, we can now move it to the final
    // location.
    let (_, final_content_path) = content_path(folder, cid);
    fs::rename(temp_file_path, &final_content_path).await?;
    tracing::info!(?final_content_path, "CAR file created");

    Ok(cid)
}

#[cfg(feature = "delia")]
mod delia_endpoints {
    use super::*;

    /// Calculate the CommP (Piece CID) for a given file.
    ///
    /// This endpoint exists as a helper for the deal-making flow because piece CID calculation
    /// requires complex operations that are difficult to perform in a browser environment.
    /// The calculation involves processing the file through IPLD chunking and computing
    /// the piece commitment, which requires specific Rust dependencies.
    ///
    /// # Usage
    /// ```http
    /// PUT /calculate_piece_cid
    /// Content-Type: multipart/form-data
    ///
    /// [file content as form data]
    /// ```
    ///
    /// # Returns
    /// Returns the calculated Piece CID as a plain text string on success.
    /// This CID is required for creating storage deals and must be included
    /// in the deal proposal.
    pub async fn calculate_piece_cid(
        ref s @ State(ref state): State<Arc<StorageServerState>>,
        request: Request,
    ) -> Result<String, (StatusCode, String)> {
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
            // Direct upload handling
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

        let (_, file_path) = content_path(&state.car_piece_storage_dir, file_cid);

        // Calculate piece commitment
        let piece_commitment_cid = tokio::task::spawn_blocking(move || -> Result<_, CommPError> {
            // Use `file_path` here instead of the undefined `piece_path`
            let (piece_commitment, _) = commp(&file_path)?;
            let piece_commitment_cid = piece_commitment.cid();
            tracing::debug!(path = %file_path.display(), commp = %piece_commitment_cid, "calculated piece commitment");
            Ok(piece_commitment_cid)
        })
        .await
        .map_err(|err| {
            tracing::error!(%err, "failed to execute blocking task");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })?
        .map_err(|err| {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })?;

        Ok(piece_commitment_cid.to_string())
    }

    /// Encode a deal proposal for signing.
    ///
    /// This endpoint exists because deal proposals must be signed by the client's private key,
    /// which is managed by the Polkadot extension in the browser. However, the exact encoding
    /// of the proposal must match what the chain expects, which requires chain-specific
    /// serialization that can't be reliably performed in JavaScript.
    ///
    /// The encoded proposal returned by this endpoint can be directly signed by the
    /// Polkadot extension and used in the `publish_deal` RPC call.
    ///
    /// # Usage
    /// ```http
    /// POST /encode_proposal
    /// Content-Type: application/json
    ///
    /// {
    ///   "client": "5...",
    ///   "provider": "5...",
    ///   "piece_cid": "baf...",
    ///   "piece_size": 1048576,
    ///   "stored_ask": {
    ///     "price_per_byte": "1",
    ///     "valid_until": 1000
    ///   }
    /// }
    /// ```
    ///
    /// # Returns
    /// Returns the SCALE-encoded proposal as a hex string, ready to be signed
    /// by the Polkadot extension.
    pub async fn encode_proposal(
        State(_): State<Arc<StorageServerState>>,
        Json(proposal): Json<SxtDealProposal>,
    ) -> Json<Result<String, String>> {
        // Convert to RuntimeDealProposal which implements Encode
        let runtime_proposal: RuntimeDealProposal<_, _, _> = proposal.into();
        // Encode the proposal - get raw bytes
        let encoded = runtime_proposal.encode();
        // Return the hex encoded proposal without the prefix
        Json(Ok(format!("0x{}", hex::encode(&encoded))))
    }
}

#[cfg(feature = "delia")]
use delia_endpoints::*;
