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
    collections::HashMap, env::temp_dir, fmt::Debug, net::SocketAddr, ops::Deref, path::PathBuf,
    sync::Arc, time::Duration,
};

use clap::Parser;
use futures::FutureExt;
use indexer::{
    local_index_directory::rdb::{RocksDBLid, RocksDBStateStoreConfig},
    start_indexer, IndexerMessage, IndexerState,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use pipeline::types::PipelineMessage;
use polka_storage_proofs::{
    porep::{self, PoRepParameters},
    post::{self, PoStParameters},
};
use polka_storage_provider_common::{config::sealing::SealingConfiguration, rpc::ServerInfo};
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use rand::Rng;
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
    task::{JoinError, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use crate::{
    config::ConfigurationArgs,
    db::{DBError, DealDB},
    pipeline::{start_pipeline, PipelineState},
    storage::{start_upload_server, StorageServerState},
};

/// Default parachain node adress.
pub(crate) const DEFAULT_NODE_ADDRESS: &str = "ws://127.0.0.1:42069";

/// Retry interval to connect to the parachain RPC.
pub(crate) const RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// Number of retries to connect to the parachain RPC.
pub(crate) const RETRY_NUMBER: u32 = 5;

/// Name for the directory where the CAR wrapped pieces are kept.
pub(crate) const CAR_PIECE_DIRECTORY_NAME: &str = "car";

/// Name for the directory where the unsealed pieces are kept.
pub(crate) const UNSEALED_SECTOR_DIRECTORY_NAME: &str = "unsealed";

/// Name for the directory where the sealed pieces are kept.
pub(crate) const SEALED_SECTOR_DIRECTORY_NAME: &str = "sealed";

/// Name for the directory where the sealing cache is kept.
pub(crate) const SEALING_CACHE_DIRECTORY_NANE: &str = "cache";

/// Name of the directory where the index is kept.
pub(crate) const INDEXER_DIRECTORY_NAME: &str = "index";

fn get_random_temporary_folder() -> PathBuf {
    temp_dir().join(
        rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(7)
            .map(char::from)
            .collect::<String>(),
    )
}

struct SetupOutput {
    storage_state: StorageServerState,
    pipeline_state: PipelineState,
    pipeline_rx: UnboundedReceiver<PipelineMessage>,
    indexer_state: IndexerState<RocksDBLid>,
    indexer_rx: UnboundedReceiver<IndexerMessage>,
}

fn main() -> Result<(), ServerError> {
    // Logger initialization.
    let file_appender = tracing_appender::rolling::daily("logs", "sp_server.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

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

    // Run requested command.
    let configuration: Server = ServerCli::parse().try_into()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the runtime")
        .block_on(configuration.run())?;

    Ok(())
}

/// CLI components error handling implementor.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("no signer keypair was passed")]
    MissingKeypair,

    #[error("storage provider is not registered")]
    UnregisteredStorageProvider,

    #[error("storage provider does not have a market account")]
    NoMarketAccountStorageProvider,

    #[error("registered proof does not match the configuration")]
    ProofMismatch,

    #[error("proof sectors sizes do not match")]
    SectorSizeMismatch,

    #[error("failed to load PoRep parameters from: {0}, because: {1}")]
    InvalidPoRepParameters(std::path::PathBuf, porep::PoRepError),

    #[error("failed to load PoSt parameters from: {0}, because: {1}")]
    InvalidPoStParameters(std::path::PathBuf, post::PoStError),

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

/// A valid server configuration. To be created using [`ServerConfiguration::try_from`].
///
/// The main difference to [`Server`] is that this structure only contains validated and
/// ready to use parameters.
pub struct Server {
    /// Storage server listen address.
    upload_listen_address: SocketAddr,

    /// Parachain node RPC url.
    node_url: Url,

    /// Storage provider key pair.
    multi_pair_signer: MultiPairSigner,

    /// Deal database directory.
    database_directory: PathBuf,

    /// Storage root directory.
    storage_directory: PathBuf,

    /// Proof of Replication proof type.
    #[allow(dead_code)] // to be removed, in the sealer implementation
    seal_proof: RegisteredSealProof,

    /// Proof of Spacetime proof type.
    post_proof: RegisteredPoStProof,

    /// Proving Parameters for PoRep proof.
    /// For 2KiB sectors they're ~1GiB of data.
    porep_parameters: PoRepParameters,

    /// Proving Parameters for PoSt proof.
    /// For 2KiB sectors they're ~11MiB of data.
    post_parameters: PoStParameters,

    /// The number of prove commits to be run in parallel.
    parallel_prove_commits: usize,

    public_secure_upload_url: Option<String>,

    /// Sealing parameters (e.g. how long to wait before sealing).
    sealing_configuration: SealingConfiguration,
}

impl Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("upload_listen_address", &self.upload_listen_address)
            .field("node_url", &self.node_url)
            .field("multi_pair_signer", &"*******")
            .field("database_directory", &self.database_directory)
            .field("storage_directory", &self.storage_directory)
            .field("seal_proof", &self.seal_proof)
            .field("post_proof", &self.post_proof)
            .field("parallel_prove_commits", &self.parallel_prove_commits)
            .field("public_secure_upload_url", &self.public_secure_upload_url)
            .field("sealing_configuration", &self.sealing_configuration)
            .finish()
    }
}

impl TryFrom<ServerCli> for Server {
    type Error = ServerError;

    fn try_from(value: ServerCli) -> Result<Self, Self::Error> {
        let config = value.config.canonicalize()?;
        let args = match config.extension() {
            Some(ext) if ext == "toml" => {
                let config = std::fs::read_to_string(config)?;
                // NOTE: without the type anotation a warning about 2024 edition is issued
                toml::from_str::<ConfigurationArgs>(&config)?
            }
            Some(ext) if ext == "json" => serde_json::from_reader(std::fs::File::open(config)?)?,
            Some(ext) => {
                println!("{:?}", ext);
                return Err(ServerError::InvalidConfig("unsupported file format"));
            }
            None => return Err(ServerError::InvalidConfig("could not detect file format")),
        };

        if args.post_proof.sector_size() != args.seal_proof.sector_size() {
            return Err(ServerError::SectorSizeMismatch);
        }

        let multi_pair_signer =
            Option::<MultiPairSigner>::from(value.multipair).ok_or(ServerError::MissingKeypair)?;

        let common_folder = get_random_temporary_folder();
        let database_directory = args.database_directory.unwrap_or_else(|| {
            let path = common_folder.join("deals_database");
            tracing::warn!(
                "no database directory was defined, using: {}",
                path.display()
            );
            path
        });
        std::fs::create_dir_all(&database_directory)?;

        let storage_directory = args.storage_directory.unwrap_or_else(|| {
            let path = common_folder.join("deals_storage");
            tracing::warn!(
                "no storage directory was defined, using: {}",
                path.display()
            );
            path
        });
        std::fs::create_dir_all(&storage_directory)?;

        let porep_parameters = args.porep_parameters;
        let porep_parameters = porep::load_groth16_parameters(porep_parameters.clone())
            .map_err(|e| ServerError::InvalidPoRepParameters(porep_parameters, e))?;

        let post_parameters = args.post_parameters;
        let post_parameters = post::load_groth16_parameters(post_parameters.clone())
            .map_err(|e| ServerError::InvalidPoStParameters(post_parameters, e))?;

        Ok(Self {
            upload_listen_address: args.upload_listen_address,
            node_url: args.node_url,
            multi_pair_signer,
            database_directory,
            storage_directory,
            seal_proof: args.seal_proof,
            post_proof: args.post_proof,
            porep_parameters,
            post_parameters,
            parallel_prove_commits: args.parallel_prove_commits.get(),
            public_secure_upload_url: args.public_secure_upload_url,
            sealing_configuration: args.sealing_configuration,
        })
    }
}

impl Server {
    pub async fn run(self) -> Result<(), ServerError> {
        info!(?self, "server configuration");

        let SetupOutput {
            storage_state,
            pipeline_state,
            pipeline_rx,
            indexer_state,
            indexer_rx,
        } = self.setup().await?;

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
                    match result {
                        Some(Ok((task_name, task_result))) => {
                            match task_result {
                                Ok(()) => tracing::info!("{task_name} finished successfully!"),
                                Err(err) => {
                                    tracing::error!("{task_name} finished with error: {err}");
                                    tracing::error!("Cancelling remaining tasks...");
                                    if error.is_none() {
                                        error = Some(err);
                                    }
                                    cancellation_token.cancel();
                                },
                            }
                        }
                        Some(Err(err)) => {
                            tracing::error!("Failed to join task with error: {err}");
                            if error.is_none() {
                                error = Some(ServerError::from(err));
                            }
                            cancellation_token.cancel();
                        }
                        None => {
                            // This branch should run when all tasks have terminated,
                            // thus, this is the most appropriate place to return the value
                            match error {
                                Some(err) => return Err(err),
                                None => return Ok(()),
                            }
                        },
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

    async fn setup(self) -> Result<SetupOutput, ServerError> {
        // Needs to be installed on top to avoid missing metrics events
        let builder = PrometheusBuilder::new();
        let exporter_handle = builder
            .install_recorder()
            .expect("Failed to install metrics recorder");

        let (xt_client, storage_provider_info) = Server::setup_storagext_client(
            self.node_url,
            &self.multi_pair_signer,
            &self.post_proof,
        )
        .await?;
        let xt_client = Arc::new(xt_client);
        let deal_database = Arc::new(DealDB::new(self.database_directory)?);

        // Car piece storage directory — i.e. the CAR archives from the input streams
        let car_piece_storage_dir = Arc::new(self.storage_directory.join(CAR_PIECE_DIRECTORY_NAME));
        let unsealed_sector_storage_dir =
            Arc::new(self.storage_directory.join(UNSEALED_SECTOR_DIRECTORY_NAME));
        let sealed_sector_storage_dir =
            Arc::new(self.storage_directory.join(SEALED_SECTOR_DIRECTORY_NAME));
        let sealing_cache_dir = Arc::new(self.storage_directory.join(SEALING_CACHE_DIRECTORY_NANE));
        let index_dir = Arc::new(self.storage_directory.join(INDEXER_DIRECTORY_NAME));

        // Create the storage directories
        tokio::fs::create_dir_all(car_piece_storage_dir.as_ref()).await?;
        tokio::fs::create_dir_all(unsealed_sector_storage_dir.as_ref()).await?;
        tokio::fs::create_dir_all(sealed_sector_storage_dir.as_ref()).await?;
        tokio::fs::create_dir_all(sealing_cache_dir.as_ref()).await?;
        tokio::fs::create_dir_all(index_dir.as_ref()).await?;

        // Channel used to action the indexer
        let (indexer_tx, indexer_rx) = tokio::sync::mpsc::unbounded_channel::<IndexerMessage>();
        // Indexer underlying database
        let lid = Arc::new(RocksDBLid::new(RocksDBStateStoreConfig {
            path: index_dir.deref().clone(),
        })?);

        let (pipeline_tx, pipeline_rx) = tokio::sync::mpsc::unbounded_channel::<PipelineMessage>();

        pipeline_tx
            .send(PipelineMessage::SchedulePoSts)
            .expect("queue not to be closed at the start-up of the server");

        let server_info = ServerInfo::new(
            self.multi_pair_signer.account_id(),
            self.seal_proof,
            self.post_proof,
            storage_provider_info.proving_period_start,
            self.sealing_configuration,
        );

        let storage_state = StorageServerState {
            server_info: server_info.clone(),
            xt_client: xt_client.clone(),
            xt_keypair: self.multi_pair_signer.clone(),
            car_piece_storage_dir: car_piece_storage_dir.clone(),
            deal_db: deal_database.clone(),
            listen_address: self.upload_listen_address,
            post_proof: self.post_proof,
            pipeline_sender: pipeline_tx.clone(),
            metrics_recorder: exporter_handle,
        };

        let pipeline_state = PipelineState {
            db: deal_database.clone(),
            server_info: server_info.clone(),
            unsealed_sectors_dir: unsealed_sector_storage_dir.clone(),
            sealed_sectors_dir: sealed_sector_storage_dir,
            sealing_cache_dir,
            porep_parameters: Arc::new(self.porep_parameters),
            post_parameters: Arc::new(self.post_parameters),
            xt_client,
            xt_keypair: self.multi_pair_signer,
            pipeline_sender: pipeline_tx,
            prove_commit_throttle: Arc::new(Semaphore::new(self.parallel_prove_commits)),
            add_piece_serializer: Mutex::new(()),
            scheduled_pre_commits: Mutex::new(HashMap::new()),
            indexer_tx,
        };

        let indexer_state = IndexerState { lid };

        Ok(SetupOutput {
            storage_state,
            pipeline_state,
            pipeline_rx,
            indexer_state,
            indexer_rx,
        })
    }

    async fn setup_storagext_client(
        rpc_address: impl AsRef<str>,
        xt_keypair: &MultiPairSigner,
        post_proof: &RegisteredPoStProof,
    ) -> Result<
        (
            storagext::Client,
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
            Some(storage_provider_info) => {
                if &storage_provider_info.info.window_post_proof_type != post_proof {
                    tracing::error!(
                        "the registered proof does not match the provided proof: {:?} != {:?}",
                        &storage_provider_info.info.window_post_proof_type,
                        post_proof
                    );
                    return Err(ServerError::ProofMismatch);
                }

                Ok((xt_client, storage_provider_info))
            }
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
}
