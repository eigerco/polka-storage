//! A CLI application that facilitates management operations over a running full node and other components.
#![warn(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]

// Explicit gate because:
// * We're not testing on non-Unix systems
// * Signal handling is very explicitly Unix-only
#[cfg(not(target_family = "unix"))]
compile_error!("polka-storage-provider-server is only compatible with Unix systems");

mod config;
mod db;
mod indexer;
mod pipeline;
mod storage;

use std::{
    collections::HashMap,
    fmt::Debug,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use clap::Parser;
use futures::FutureExt;
use indexer::{
    local_index_directory::rdb::{RocksDBLid, RocksDBStateStoreConfig},
    start_indexer, IndexerMessage, IndexerState,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use pipeline::types::PipelineMessage;
use polka_storage_proofs::{
    porep::{self},
    post::{self},
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives::proofs::RegisteredPoStProof;
use storagext::{
    multipair::{MultiPairArgs, MultiPairSigner},
    runtime::runtime_types::{
        bounded_collections::bounded_vec::BoundedVec,
        pallet_storage_provider::storage_provider::StorageProviderState,
    },
    BlockNumber, StorageProviderClientExt,
};
use subxt::{self, tx::Signer};
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::{mpsc::UnboundedReceiver, Mutex, Semaphore},
    task::{spawn_blocking, JoinError, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    config::ServerConfiguration,
    db::{DBError, DealDB},
    pipeline::{start_pipeline, PipelineState},
    storage::{start_upload_server, StorageServerState},
};

/// Retry interval to connect to the parachain RPC.
pub(crate) const RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// Number of retries to connect to the parachain RPC.
pub(crate) const RETRY_NUMBER: u32 = 5;

/// Name for the directory where the CAR wrapped pieces are kept.
const CAR_PIECE_DIRECTORY_NAME: &str = "car";

/// Name for the directory where the unsealed pieces are kept.
const UNSEALED_SECTOR_DIRECTORY_NAME: &str = "unsealed";

/// Name for the directory where the sealed pieces are kept.
const SEALED_SECTOR_DIRECTORY_NAME: &str = "sealed";

/// Name for the directory where the sealing cache is kept.
const SEALING_CACHE_DIRECTORY_NANE: &str = "cache";

/// Name of the directory where the index is kept.
const INDEXER_DIRECTORY_NAME: &str = "index";

/// CLI components error handling implementer.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("no signer keypair was passed")]
    MissingKeypair,

    #[error("storage provider is not registered")]
    UnregisteredStorageProvider,

    #[error("registered proof does not match the configuration")]
    ProofMismatch,

    #[error("proof sectors sizes do not match")]
    SectorSizeMismatch,

    #[error(transparent)]
    InvalidPoRepParameters(#[from] porep::PoRepError),

    #[error(transparent)]
    InvalidPoStParameters(#[from] post::PoStError),

    #[error("FromEnv error: {0}")]
    EnvFilter(#[from] tracing_subscriber::filter::FromEnvError),

    #[error("URL parse error: {0}")]
    ParseUrl(#[from] url::ParseError),

    #[error("Error occurred while working with a car file: {0}")]
    Mater(#[from] mater::Error),

    #[error(transparent)]
    Subxt(#[from] subxt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Db(#[from] DBError),

    #[error(transparent)]
    Join(#[from] JoinError),

    #[error("Invalid config: {0}")]
    InvalidConfig(&'static str),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Lid(#[from] crate::indexer::local_index_directory::LidError),
}

/// The server arguments, as passed by the user, unvalidated.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
pub struct ServerCli {
    // Shorthand for all the keys
    #[command(flatten)]
    multipair: MultiPairArgs,

    /// Path to the server configuration file.
    #[arg(long)]
    config: PathBuf,
}

struct StorageDirectories {
    car_piece_storage_dir: PathBuf,
    unsealed_sectors_dir: PathBuf,
    sealed_sectors_dir: PathBuf,
    sealing_cache_dir: PathBuf,
    index_dir: PathBuf,
}

impl StorageDirectories {
    async fn prepare_storage_directories<P>(storage_directory: P) -> Result<Self, io::Error>
    where
        P: AsRef<Path>,
    {
        let car_piece_storage_dir = storage_directory.as_ref().join(CAR_PIECE_DIRECTORY_NAME);
        let unsealed_sectors_dir = storage_directory
            .as_ref()
            .join(UNSEALED_SECTOR_DIRECTORY_NAME);
        let sealed_sectors_dir = storage_directory
            .as_ref()
            .join(SEALED_SECTOR_DIRECTORY_NAME);
        let sealing_cache_dir = storage_directory
            .as_ref()
            .join(SEALING_CACHE_DIRECTORY_NANE);
        let index_dir = storage_directory.as_ref().join(INDEXER_DIRECTORY_NAME);

        // Create the storage directories
        tokio::fs::create_dir_all(&car_piece_storage_dir).await?;
        tokio::fs::create_dir_all(&unsealed_sectors_dir).await?;
        tokio::fs::create_dir_all(&sealed_sectors_dir).await?;
        tokio::fs::create_dir_all(&sealing_cache_dir).await?;
        tokio::fs::create_dir_all(&index_dir).await?;

        Ok(Self {
            car_piece_storage_dir,
            unsealed_sectors_dir,
            sealed_sectors_dir,
            sealing_cache_dir,
            index_dir,
        })
    }
}

struct ServerComponents {
    storage_state: StorageServerState,
    pipeline_state: PipelineState,
    pipeline_rx: UnboundedReceiver<PipelineMessage>,
    indexer_state: IndexerState<RocksDBLid>,
    indexer_rx: UnboundedReceiver<IndexerMessage>,
}

impl ServerComponents {
    async fn from_config(
        config: ServerConfiguration,
        multi_pair_signer: MultiPairSigner,
        metrics_recorder: PrometheusHandle,
    ) -> Result<Self, ServerError> {
        tokio::fs::create_dir_all(&config.database_directory).await?;
        tokio::fs::create_dir_all(&config.storage_directory).await?;

        let (xt_client, storage_provider_info) =
            setup_storagext_client(config.node_url, &multi_pair_signer, config.post_proof).await?;

        let StorageDirectories {
            car_piece_storage_dir,
            unsealed_sectors_dir,
            sealed_sectors_dir,
            sealing_cache_dir,
            index_dir,
        } = StorageDirectories::prepare_storage_directories(config.storage_directory).await?;

        let server_info = ServerInfo::new(
            multi_pair_signer.account_id(),
            config.seal_proof,
            config.post_proof,
            storage_provider_info.proving_period_start,
            config.sealing_configuration,
        );

        let deal_database = Arc::new(DealDB::new(config.database_directory)?);
        let (pipeline_tx, pipeline_rx) = tokio::sync::mpsc::unbounded_channel::<PipelineMessage>();
        pipeline_tx
            .send(PipelineMessage::SchedulePoSts)
            .expect("queue not to be closed at the start-up of the server");

        let storage_state = StorageServerState {
            server_info: server_info.clone(),
            xt_client: xt_client.clone(),
            xt_keypair: multi_pair_signer.clone(),
            car_piece_storage_dir,
            deal_db: deal_database.clone(),
            listen_address: config.listen_address,
            post_proof: config.post_proof,
            pipeline_sender: pipeline_tx.clone(),
            metrics_recorder,
        };

        let porep_parameters = spawn_blocking(|| {
            tracing::debug!(
                "Loading PoRep parameters from {}",
                config.porep_parameters.display()
            );
            porep::load_groth16_parameters(config.porep_parameters)
        })
        .await??;

        let post_parameters = spawn_blocking(|| {
            tracing::debug!(
                "Loading PoSt parameters from {}",
                config.post_parameters.display()
            );
            post::load_groth16_parameters(config.post_parameters)
        })
        .await??;

        // Channel used to action the indexer
        let (indexer_tx, indexer_rx) = tokio::sync::mpsc::unbounded_channel::<IndexerMessage>();
        let pipeline_state = PipelineState {
            db: deal_database.clone(),
            server_info: server_info.clone(),
            unsealed_sectors_dir,
            sealed_sectors_dir,
            sealing_cache_dir,
            porep_parameters: Arc::new(porep_parameters),
            post_parameters: Arc::new(post_parameters),
            xt_client,
            xt_keypair: multi_pair_signer,
            pipeline_sender: pipeline_tx,
            prove_commit_throttle: Arc::new(Semaphore::new(config.parallel_prove_commits.get())),
            add_piece_serializer: Mutex::new(()),
            scheduled_pre_commits: Mutex::new(HashMap::new()),
            indexer_tx,
        };

        // Indexer underlying database
        let lid = Arc::new(RocksDBLid::new(RocksDBStateStoreConfig {
            path: index_dir,
        })?);
        let indexer_state = IndexerState { lid };

        Ok(ServerComponents {
            storage_state,
            pipeline_state,
            pipeline_rx,
            indexer_state,
            indexer_rx,
        })
    }
}

// TODO: figure out the way to write the setup ceremony

async fn setup_storagext_client(
    rpc_address: impl AsRef<str>,
    xt_keypair: &MultiPairSigner,
    post_proof: RegisteredPoStProof,
) -> Result<
    (
        Arc<storagext::Client>,
        StorageProviderState<BoundedVec<u8>, u128, BlockNumber>,
    ),
    ServerError,
> {
    let xt_client = storagext::Client::new(rpc_address, RETRY_NUMBER, RETRY_INTERVAL).await?;

    let storage_provider_account_id = subxt::utils::AccountId32(xt_keypair.account_id().into());

    // Check if the storage provider has been registered to the chain
    let storage_provider_info = xt_client
        .retrieve_storage_provider(&storage_provider_account_id)
        .await?;

    match storage_provider_info {
        Some(storage_provider_info)
            if storage_provider_info.info.window_post_proof_type != post_proof =>
        {
            tracing::error!(
                "the registered proof does not match the provided proof: {:?} != {:?}",
                storage_provider_info.info.window_post_proof_type,
                post_proof
            );
            Err(ServerError::ProofMismatch)
        }
        Some(storage_provider_info) => Ok((Arc::new(xt_client), storage_provider_info)),
        None => {
            tracing::error!(concat!(
                "the provider key did not match a registered account id, ",
                "you can register your account using ",
                "`storagext-cli storage-provider register`"
            ));

            Err(ServerError::UnregisteredStorageProvider)
        }
    }
}

async fn run(
    ServerComponents {
        storage_state,
        pipeline_state,
        pipeline_rx,
        indexer_state,
        indexer_rx,
    }: ServerComponents,
) -> Result<(), ServerError> {
    let cancellation_token = CancellationToken::new();

    let mut tasks = JoinSet::new();
    tasks.spawn(
        start_upload_server(Arc::new(storage_state), cancellation_token.child_token())
            .map(|result| ("HTTP Storage", result.map_err(ServerError::from))),
    );
    tasks.spawn(
        start_pipeline(
            Arc::new(pipeline_state),
            pipeline_rx,
            cancellation_token.child_token(),
        )
        .map(|result| ("Pipeline", result.map_err(ServerError::from))),
    );
    tasks.spawn(
        start_indexer(indexer_state, indexer_rx, cancellation_token.child_token())
            .map(|result| ("Indexer", result.map_err(ServerError::from))),
    );
    tracing::info!("Successfully launched all sub-services, ready for work!");

    // Infos here:
    // https://www.gnu.org/software/libc/manual/html_node/Termination-Signals.html
    let mut sigint_listener =
        signal(SignalKind::interrupt()).expect("should be able to listen for SIGINT");
    let mut sigterm_listener =
        signal(SignalKind::terminate()).expect("should be able to listen for SIGTERM");

    // Keep the first error around as the "canonical return",
    // since we can't return multiple values
    let mut error = None;
    loop {
        tokio::select! {
            result = tasks.join_next() => {
                let Some(result) = result else {
                    // This branch should run when all tasks have terminated,
                    // thus, this is the most appropriate place to return the value
                    return match error {
                        Some(err) => Err(err),
                        None => Ok(()),
                    }
                };
                match result {
                    Ok((task_name, Ok(()))) => tracing::info!("{task_name} finished successfully!"),
                    Ok((task_name, Err(err))) => {
                        tracing::error!("{task_name} finished with error: {err}");
                        tracing::error!("Cancelling remaining tasks...");
                        if error.is_none() {
                            error = Some(err);
                        }
                        cancellation_token.cancel();
                    }
                    Err(err) => {
                        tracing::error!("Failed to join task with error: {err}");
                        if error.is_none() {
                            error = Some(ServerError::from(err));
                        }
                        cancellation_token.cancel();
                    }
                }
            }
            _ = sigint_listener.recv() => {
                tracing::info!("SIGINT received, shutting down...");
                cancellation_token.cancel();
            }
            _ = sigterm_listener.recv() => {
                tracing::info!("SIGTERM received, shutting down...");
                cancellation_token.cancel();
            }
        }
    }
}

fn main() -> Result<(), ServerError> {
    // Metrics initialization.
    let builder = PrometheusBuilder::new();
    let exporter_handle = builder
        .install_recorder()
        .expect("Failed to install metrics recorder");

    // Logger initialization.
    let file_appender = tracing_appender::rolling::daily("logs", "sp_server.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // File-logging initialization
    tracing_subscriber::registry()
        // File appender *MUST* go first, otherwise `with_ansi` isn't respected
        // More info: https://github.com/tokio-rs/tracing/issues/3089
        .with(fmt::layer().with_ansi(false).with_writer(non_blocking))
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .init();

    let ServerCli { multipair, config } = ServerCli::parse();
    let multi_pair_signer =
        Option::<MultiPairSigner>::from(multipair).ok_or(ServerError::MissingKeypair)?;
    let config = ServerConfiguration::from_path(config)?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the runtime")
        .block_on(async {
            let components =
                ServerComponents::from_config(config, multi_pair_signer, exporter_handle).await?;
            run(components).await
        })?;

    Ok(())
}
