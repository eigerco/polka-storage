use std::{
    io::{self, ErrorKind},
    net::SocketAddr,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, MatchedPath, Multipart, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use futures::{TryFutureExt, TryStreamExt};
use hyper::{header::CONTENT_TYPE, Method};
use mater::Cid;
use metrics_exporter_prometheus::PrometheusHandle;
use polka_storage_provider_common::{
    commp::{commp, CommPError},
    rpc::{CidString, ServerInfo},
};
use primitives::{
    commitment::{piece::PaddedPieceSize, CommP, Commitment, CommitmentKind},
    proofs::RegisteredPoStProof,
};
use storagext::{
    types::storage_provider::{
        ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal,
    },
    StorageProviderClientExt, SystemClientExt,
};
use subxt::tx::Signer;
use tokio::{
    fs::{self, File},
    io::{AsyncRead, BufWriter},
    sync::mpsc::UnboundedSender,
};
use tokio_util::{io::StreamReader, sync::CancellationToken};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    db::DealDB,
    pipeline::types::{AddPieceMessage, PipelineMessage},
};

/// Shared state of the storage server.
pub struct StorageServerState {
    pub server_info: ServerInfo,
    pub car_piece_storage_dir: PathBuf,

    pub deal_db: Arc<DealDB>,

    pub listen_address: SocketAddr,

    pub xt_client: Arc<storagext::Client>,
    pub xt_keypair: storagext::multipair::MultiPairSigner,

    // I think this just needs the sector size actually
    #[allow(dead_code)]
    pub post_proof: RegisteredPoStProof,

    pub pipeline_sender: UnboundedSender<PipelineMessage>,

    pub metrics_recorder: PrometheusHandle,
}

#[tracing::instrument(skip_all)]
pub async fn start_upload_server(
    state: Arc<StorageServerState>,
    token: CancellationToken,
) -> Result<(), std::io::Error> {
    // Create a storage folder if it doesn't exist.
    if !state.car_piece_storage_dir.exists() {
        tracing::info!(folder = ?state.car_piece_storage_dir, "creating storage folder");
        fs::create_dir_all(&state.car_piece_storage_dir).await?;
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
    // NOTE(@jmg-duarte,12/03/2025): this will need extra checking but for now it's ok
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE])
        .max_age(std::time::Duration::from_secs(3600));

    Router::new()
        .route(
            "/api/v0/upload/:cid",
            put(upload)
                // Limit upload size to maximum sector size
                .layer(DefaultBodyLimit::max(
                    // Cast is safe because we're not supporting 32-bit systems
                    state.post_proof.sector_size().bytes() as usize,
                )),
        )
        .route("/api/v0/download/:cid", get(download))
        .route("/api/v0/propose_deal", post(propose_deal))
        .route("/api/v0/publish_deal", post(publish_deal))
        .route("/metrics", get(metrics))
        .with_state(state)
        .layer(cors)
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

async fn metrics(
    State(state): State<Arc<StorageServerState>>,
) -> Result<Response<String>, (StatusCode, String)> {
    // HACK: while we dont separate the pre-commited sectors from proven
    // will count the number of active sectors and report it to metrics
    // Not super performant because it does a linear scan over the CF
    state.deal_db.measure_active_sectors();

    Response::builder()
        .status(200)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(state.metrics_recorder.render())
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
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
        stream_contents_to_car(&state.car_piece_storage_dir, field_reader)
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
        stream_contents_to_car(&state.car_piece_storage_dir, body_reader)
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
        let (piece_commitment, _) = commp(&piece_path)?;
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

    // Open car file
    let file = File::open(&path).await.map_err(|e| {
        if let ErrorKind::NotFound = e.kind() {
            tracing::error!(?path, "file not found");
            (StatusCode::NOT_FOUND, "file not found".to_string())
        } else {
            tracing::error!(?e, ?path, "failed to open file");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to open file".to_string(),
            )
        }
    })?;

    let mut reader = mater::FileReader::new(file)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let root = *reader
        .roots()
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .get(0)
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "malformed or corrupted file".to_string(),
        ))?;
    let body = Body::from_stream(reader.chunk_stream(root));

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
fn content_path<P>(folder: P, cid: Cid) -> (String, PathBuf)
where
    P: AsRef<std::path::Path>,
{
    let name = format!("{cid}.car");
    let path = folder.as_ref().join(&name);
    (name, path)
}

/// Reads bytes from the source and writes them to a CAR file.
async fn stream_contents_to_car<R, P>(
    folder: P,
    source: R,
) -> Result<Cid, Box<dyn std::error::Error>>
where
    R: AsyncRead + Unpin,
    P: AsRef<std::path::Path>,
{
    // Temp file which will be used to store the CAR file content. The temp
    // director has a randomized name and is created in the same folder as the
    // finalized uploads are stored.
    let temp_dir = tempfile::tempdir_in(folder.as_ref())?;
    let temp_file_path = temp_dir.path().join("temp.car");
    tracing::trace!("writing file to {}", temp_file_path.display());

    // Stream the body from source to the temp file.
    let file = File::create(&temp_file_path).await?;
    let writer = BufWriter::new(file);
    let cid = mater::FileWriter::import(source, writer).await?;
    tracing::trace!("finished writing the CAR archive");

    // If the file is successfully written, we can now move it to the final
    // location.
    let (_, final_content_path) = content_path(folder.as_ref(), cid);
    fs::rename(temp_file_path, &final_content_path).await?;
    tracing::info!(?final_content_path, "CAR file created");

    Ok(cid)
}

async fn validate_deal_proposal(
    state: Arc<StorageServerState>,
    deal: &SxtDealProposal,
) -> Result<(), (StatusCode, String)> {
    if deal.start_block > deal.end_block {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Deal's start block cannot be after end block: start_block = {}, end_block = {}",
                deal.start_block, deal.end_block
            ),
        ));
    }

    let current_block = state
        .xt_client
        .height(true)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    if current_block > deal.start_block {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Deal starts in the past: current_block = {}, deal_start_block = {}",
                current_block, deal.start_block
            ),
        ));
    }

    let deal_start_distance = deal.start_block - current_block;
    let minimum_start_distance = state
        .server_info
        .sealing_configuration
        .minimum_start_distance();
    // NOTE(@jmg-duarte,12/02/2025): we could consider the deal size when doing this,
    // if a deal is going to fill up a single sector, we could let it through as long as
    // its deal_start_distance > pre_commit_submission_slack
    if deal_start_distance < minimum_start_distance {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Deal starts too early: start_block = {}, (current) minimum_start_block = {}",
                deal.start_block,
                current_block + minimum_start_distance,
            ),
        ));
    }

    // We don't check for the minimum expiration because:
    // * A future deal may come that makes the sector valid
    // * We can always set the sector lifetime to match the minimum at the expense of the SP
    // TODO(@jmg-duarte,05/02/2025): Check what Filecoin does in this case

    // When adding a piece/deal to a sector, we must ensure the sector remains valid
    // i.e. no invariants are broken; as such we must ensure that the deal being added
    // does not expire beyond the maximum sector expiration.
    //
    // NOTE(@jmg-duarte,31/01/2025): there's an hidden issue here that we can't address just now
    // the min/max sector expirations are moving targets, calculated from the current block
    // this means that we can only truly validate the invariants when submitting the pre-commit
    // Only when addressing issue #671 we will be able to fully solve this, since as soon as a deal
    // is added to a sector the clock starts ticking, if we wait too long the minimum expiration
    // may itself "expire".
    // The FC codebase doesn't really have any clues how this is solved, being probably left as an
    // "invisible" agreement between the client and SP that it just should work
    // The most useful piece of source is in:
    // https://github.com/filecoin-project/lotus/blob/a526c480d40898a079c806748639e8db07aa2298/storage/pipeline/input.go#L566
    let (_, max_sector_expiration) = state
        .xt_client
        .sector_expiration_bounds()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    // We know that this doesn't underflow because we know that:
    // * deal.start_block > current_block
    // * deal.start_block < deal.end_block
    // As such, deal.end_block > current_block
    let deal_end_distance = deal.end_block - current_block;
    if deal_end_distance > max_sector_expiration {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Deal expiration is beyond the maximum accepted limit: deal.end_block = {}, (current) expiration_limit_block = {}",
                deal.end_block,
                current_block + max_sector_expiration
            ),
        ));
    }

    let post_sector_size = state.server_info.post_proof.sector_size().bytes();
    if deal.piece_size > post_sector_size {
        return Err(
            (
                StatusCode::BAD_REQUEST,
                format!(
                    "Deal piece size is larger than the supported sector size: piece_size = {}, sector_size = {}",
                    deal.piece_size, post_sector_size
                )
            )
        );
    }

    let provider_id = state.xt_keypair.account_id();
    if deal.provider != provider_id {
        return Err((StatusCode::BAD_REQUEST,
            format!(
                "Deal provider does not match current provider: deal_provider_id = {}, current_provider_id = {}",
                deal.provider, provider_id
            )));
    }

    let piece_cid_codec = deal.piece_cid.codec();
    let commp_codec = CommP::multicodec();
    if piece_cid_codec != commp_codec {
        return Err((StatusCode::BAD_REQUEST,
            format!(
                "Piece's CID codec is not a piece commitment: piece_cid_codec = {}, commitment_cid_codec = {}",
                piece_cid_codec, commp_codec
            )));
    }

    if !deal.piece_size.is_power_of_two() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Deal's piece size not a power of two: piece_size = {}",
                deal.piece_size
            ),
        ));
    }

    if deal.storage_price_per_block == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Price per block must be greater than 0".to_string(),
        ));
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(deal))]
async fn propose_deal(
    State(state): State<Arc<StorageServerState>>,
    Json(deal): Json<SxtDealProposal>,
) -> Result<Json<CidString>, (StatusCode, String)> {
    validate_deal_proposal(state.clone(), &deal).await?;
    let cid = state
        .deal_db
        .add_accepted_proposed_deal(&deal)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(CidString::from(cid)))
}

#[tracing::instrument(skip_all, fields(deal))]
async fn publish_deal(
    State(state): State<Arc<StorageServerState>>,
    Json(deal): Json<SxtClientDealProposal>,
) -> Result<Json<u64>, (StatusCode, String)> {
    if deal.deal_proposal.piece_size > state.server_info.post_proof.sector_size().bytes() {
        // once again, the rpc error is wrong, we'll need to fix that
        return Err((
            StatusCode::BAD_REQUEST,
            "Piece size cannot be larger than the registered sector size".to_string(),
        ));
    }

    let deal_proposal_cid = deal
        .deal_proposal
        .json_cid()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    // Check if this deal proposal has been accepted or not, error if not
    if state
        .deal_db
        .get_proposed_deal(deal_proposal_cid)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .is_none()
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Proposal has not been found — have you proposed the deal first?".to_string(),
        ));
    }

    // Check if the respective piece has been uploaded, error if not
    let piece_cid = deal.deal_proposal.piece_cid;
    let piece_path = state.car_piece_storage_dir.join(format!("{piece_cid}.car"));
    if !piece_path.exists() || !piece_path.is_file() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Piece has not been uploaded yet".to_string(),
        ));
    }

    // TODO(@jmg-duarte,25/11/2024): don't batch the deals for better errors

    let deal_proposal = deal.deal_proposal.clone();
    // TODO(@jmg-duarte,#428,04/10/2024):
    // There's a small bug here, currently, xt_client waits for a "full extrisic submission"
    // meaning that it will wait until the block where it is included in is finalized
    // however, due to https://github.com/paritytech/subxt/issues/1668 it may wrongly fail.
    // Fixing this requires the xt_client not wait for the finalization, it's not hard to do
    // it just requires some API design
    let result = state
        .xt_client
        .publish_signed_storage_deals(&state.xt_keypair, vec![deal], true)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
        .await?
        .expect("we're waiting for the finalization so it should NEVER be None");

    let published_deals = result
        .events
        .find_first::<storagext::runtime::storage_provider::events::DealsPublished>()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let Some(published_deals) = published_deals else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to find any published deals".to_string(),
        ));
    };

    // We currently just support a single deal and if there's no published deals,
    // an error MUST've happened
    debug_assert_eq!(published_deals.deals.0.len(), 1);

    // We always publish only 1 deal
    let deal_id = published_deals
        .deals
        .0
        .first()
        .expect("we only support a single deal")
        .deal_id;

    let commitment = Commitment::from_cid(&piece_cid)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    state
        .pipeline_sender
        .send(PipelineMessage::AddPiece(AddPieceMessage {
            deal: deal_proposal,
            published_deal_id: deal_id,
            piece_path,
            commitment,
        }))
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(deal_id))
}
