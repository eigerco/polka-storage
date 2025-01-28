//! A CLI application that facilitates management operations over a running full node and other components.
#![warn(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]

mod config;
mod db;
mod local_index_directory;
mod p2p;
mod pipeline;
mod retrieval;
mod rpc;
mod storage;

use std::{env::temp_dir, net::SocketAddr, ops::Deref, path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use libp2p::{identity::Keypair, Multiaddr, PeerId};
use local_index_directory::rdb::{RocksDBLid, RocksDBStateStoreConfig};
use p2p::{
    run_bootstrap_node, run_register_node, BootstrapConfig, NodeType, P2PError, P2PState,
    RegisterConfig,
};
use pipeline::types::PipelineMessage;
use polka_storage_proofs::{
    porep::{self, PoRepParameters},
    post::{self, PoStParameters},
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use rand::Rng;
use retrieval::{start_retrieval, RetrievalServerConfig};
use storagext::{
    multipair::{MultiPairArgs, MultiPairSigner},
    runtime::runtime_types::{
        bounded_collections::bounded_vec::BoundedVec,
        pallet_storage_provider::storage_provider::StorageProviderState,
    },
    MarketClientExt, StorageProviderClientExt,
};
use subxt::{self, tx::Signer};
use tokio::{
    sync::{mpsc::UnboundedReceiver, Semaphore},
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use crate::{
    config::ConfigurationArgs,
    db::{DBError, DealDB},
    pipeline::{start_pipeline, PipelineState},
    rpc::{start_rpc_server, RpcServerState},
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
    rpc_state: RpcServerState,
    pipeline_state: PipelineState,
    pipeline_rx: UnboundedReceiver<PipelineMessage>,
    p2p_state: P2PState,
    retrieval_config: RetrievalServerConfig<RocksDBLid>,
}

fn main() -> Result<(), ServerError> {
    // Logger initialization.
    let file_appender = tracing_appender::rolling::daily("logs", "sp_server");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
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

    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),

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
    P2P(#[from] P2PError),

    #[error(transparent)]
    RetrievalServer(#[from] polka_storage_retrieval::server::ServerError),

    #[error(transparent)]
    Lid(#[from] crate::local_index_directory::LidError),
}

/// Takes an expression that returns a `Result<Result<T, E2>, E1>`.
/// It tries to inspect and log the first error (`E1`), otherwise,
/// it inspects the result and tries to inspect the nested error (`E2`).
///
/// This macro is *roughly* equivalent to calling:
/// ```text
/// res // : Result<Result<T, E2>, E1>
///     .inspect_err(|e| tracing::error!(%e))
///     .inspect(|r| r.inspect_err(|e| tracing::error!(%e))
/// ```
macro_rules! inspect_and_log_nested_errors {
    ($($task:expr),+ $(,)?) => {
        (
            $(
                $task
                    .inspect_err(|err| tracing::error!(%err))
                    .inspect(|ok| {
                        let _ = ok.as_ref().inspect_err(|err| tracing::error!(%err));
                    })
            ),+
        )
    };
}

/// The server arguments, as passed by the user, unvalidated.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
pub struct ServerCli {
    // Shorthand for all the keys
    #[command(flatten)]
    multipair: MultiPairArgs,

    #[command(flatten)]
    args: Option<ConfigurationArgs>,

    /// Path to the server configuration file.
    #[arg(long)]
    config: Option<PathBuf>,
}

/// A valid server configuration. To be created using [`ServerConfiguration::try_from`].
///
/// The main difference to [`Server`] is that this structure only contains validated and
/// ready to use parameters.
pub struct Server {
    /// Storage server listen address.
    upload_listen_address: SocketAddr,

    /// RPC server listen address.
    rpc_listen_address: SocketAddr,

    /// Parachain node RPC url.
    node_url: Url,

    /// Storage provider listen address.
    retrieval_listen_address: Multiaddr,

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

    /// P2P Network node type, can either be a bootstrap or registration node
    node_type: NodeType,

    /// P2P ED25519 private key
    p2p_key: Keypair,

    /// Rendezvous point address that the registration node connects to
    /// or the bootstrap node binds to.
    rendezvous_point_address: Multiaddr,

    /// PeerID of the bootstrap node used by the registration node.
    /// Optional because it is not used by the bootstrap node.
    rendezvous_point: Option<PeerId>,

    /// TTL of the p2p registration in seconds
    registration_ttl: u64,
}

impl TryFrom<ServerCli> for Server {
    type Error = ServerError;

    fn try_from(value: ServerCli) -> Result<Self, Self::Error> {
        let args: ConfigurationArgs = if let Some(config) = value.config {
            let config = config.canonicalize()?;
            match config.extension() {
                Some(ext) if ext == "toml" => {
                    let config = std::fs::read_to_string(config)?;
                    // NOTE: without the type anotation a warning about 2024 edition is issued
                    toml::from_str::<ConfigurationArgs>(&config)?
                }
                Some(ext) if ext == "json" => {
                    serde_json::from_reader(std::fs::File::open(config)?)?
                }
                Some(ext) => {
                    println!("{:?}", ext);
                    return Err(ServerError::InvalidConfig("unsupported file format"));
                }
                None => return Err(ServerError::InvalidConfig("could not detect file format")),
            }
        } else {
            value.args.expect(
                "if `config == None` and `args_required_else_help = true`, then args must be Some",
            )
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
            rpc_listen_address: args.rpc_listen_address,
            node_url: args.node_url,
            multi_pair_signer,
            database_directory,
            storage_directory,
            seal_proof: args.seal_proof,
            post_proof: args.post_proof,
            porep_parameters,
            post_parameters,
            parallel_prove_commits: args.parallel_prove_commits.get(),
            node_type: args.node_type,
            p2p_key: args.p2p_key,
            rendezvous_point_address: args.rendezvous_point_address,
            rendezvous_point: args.rendezvous_point,
            registration_ttl: args.registration_ttl,
            retrieval_listen_address: args.retrieval_listen_address,
        })
    }
}

impl Server {
    pub async fn run(self) -> Result<(), ServerError> {
        let SetupOutput {
            storage_state,
            rpc_state,
            pipeline_state,
            pipeline_rx,
            p2p_state,
            retrieval_config,
        } = self.setup().await?;

        let cancellation_token = CancellationToken::new();

        let p2p_task = spawn_p2p_task(p2p_state, cancellation_token.child_token())?;
        let rpc_task = tokio::spawn(start_rpc_server(
            rpc_state,
            cancellation_token.child_token(),
        ));
        let storage_task = tokio::spawn(start_upload_server(
            Arc::new(storage_state),
            cancellation_token.child_token(),
        ));
        let pipeline_task = tokio::spawn(start_pipeline(
            Arc::new(pipeline_state),
            pipeline_rx,
            cancellation_token.child_token(),
        ));

        let retrieval_task = tokio::spawn(start_retrieval(
            retrieval_config,
            cancellation_token.child_token(),
        ));

        // Wait for SIGTERM on the main thread and once received "unblock"
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for event");
        tracing::info!("SIGTERM received, shutting down...");

        cancellation_token.cancel();
        tracing::info!("sent shutdown signal");

        // Wait for the tasks to finish
        let (upload_result, rpc_task, pipeline_task, p2p_task, retrieval_task) = tokio::join!(
            storage_task,
            rpc_task,
            pipeline_task,
            p2p_task,
            retrieval_task
        );

        // Inspect and log errors
        let (upload_result, rpc_task, pipeline_task, p2p_task) =
            inspect_and_log_nested_errors!(upload_result, rpc_task, pipeline_task, p2p_task);

        // Exit with error
        upload_result??;
        rpc_task??;
        pipeline_task??;
        p2p_task??;
        retrieval_task??;

        Ok(())
    }

    async fn setup(self) -> Result<SetupOutput, ServerError> {
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

        // Indexer used by the system
        let indexer = Arc::new(RocksDBLid::new(RocksDBStateStoreConfig {
            path: index_dir.deref().clone(),
        })?);

        let (pipeline_tx, pipeline_rx) = tokio::sync::mpsc::unbounded_channel::<PipelineMessage>();

        pipeline_tx
            .send(PipelineMessage::SchedulePoSts)
            .expect("queue not to be closed at the start-up of the server");

        let storage_state = StorageServerState {
            car_piece_storage_dir: car_piece_storage_dir.clone(),
            deal_db: deal_database.clone(),
            listen_address: self.upload_listen_address,
            post_proof: self.post_proof,
        };

        let rpc_state = RpcServerState {
            server_info: ServerInfo::new(
                self.multi_pair_signer.account_id(),
                self.seal_proof,
                self.post_proof,
                storage_provider_info.proving_period_start,
            ),
            deal_db: deal_database.clone(),
            car_piece_storage_dir: car_piece_storage_dir.clone(),
            xt_client: xt_client.clone(),
            xt_keypair: self.multi_pair_signer.clone(),
            listen_address: self.rpc_listen_address,
            pipeline_sender: pipeline_tx.clone(),
        };

        let pipeline_state = PipelineState {
            db: deal_database.clone(),
            server_info: rpc_state.server_info.clone(),
            unsealed_sectors_dir: unsealed_sector_storage_dir.clone(),
            sealed_sectors_dir: sealed_sector_storage_dir,
            sealing_cache_dir,
            porep_parameters: Arc::new(self.porep_parameters),
            post_parameters: Arc::new(self.post_parameters),
            xt_client,
            xt_keypair: self.multi_pair_signer,
            pipeline_sender: pipeline_tx,
            prove_commit_throttle: Arc::new(Semaphore::new(self.parallel_prove_commits)),
        };

        let p2p_state = P2PState {
            node_type: self.node_type,
            p2p_key: self.p2p_key,
            rendezvous_point_address: self.rendezvous_point_address,
            rendezvous_point: self.rendezvous_point,
            registration_ttl: self.registration_ttl,
        };

        let unsealed_sectors_dir = unsealed_sector_storage_dir.deref().clone();
        let retrieval_config = RetrievalServerConfig {
            listen_address: self.retrieval_listen_address,
            unsealed_sectors_dir,
            indexer,
        };

        Ok(SetupOutput {
            storage_state,
            rpc_state,
            pipeline_state,
            pipeline_rx,
            p2p_state,
            retrieval_config,
        })
    }

    async fn setup_storagext_client(
        rpc_address: impl AsRef<str>,
        xt_keypair: &MultiPairSigner,
        post_proof: &RegisteredPoStProof,
    ) -> Result<
        (
            storagext::Client,
            StorageProviderState<BoundedVec<u8>, u128, u64>,
        ),
        ServerError,
    > {
        let xt_client = storagext::Client::new(rpc_address, RETRY_NUMBER, RETRY_INTERVAL).await?;

        let storage_provider_account_id = xt_keypair.account_id().into();

        // Check if the storage provider has been registered to the chain
        let storage_provider_info = xt_client
            .retrieve_storage_provider(&storage_provider_account_id)
            .await?;

        // Check if the account exists on the market
        xt_client
            // Once subxt breaks our code with https://github.com/paritytech/subxt/pull/1850
            // we'll be able to make all this uniform
            .retrieve_balance(subxt::ext::sp_runtime::AccountId32::new(
                storage_provider_account_id.0,
            ))
            .await?
            .ok_or(ServerError::NoMarketAccountStorageProvider)?;

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

/// Spawns a p2p node and returns a `JoinHandle`.
/// The node type is either bootstrap or registration depending on the `p2p_state.node_type` value.
fn spawn_p2p_task(
    p2p_state: P2PState,
    cancellation_token: CancellationToken,
) -> Result<JoinHandle<Result<(), P2PError>>, ServerError> {
    match p2p_state.node_type {
        NodeType::Bootstrap => {
            let config =
                BootstrapConfig::new(p2p_state.p2p_key, p2p_state.rendezvous_point_address);
            Ok(tokio::spawn(run_bootstrap_node(config, cancellation_token)))
        }
        NodeType::Register => {
            let Some(rendezvous_point) = p2p_state.rendezvous_point else {
                return Err(ServerError::P2P(P2PError::InvalidBehaviourConfig));
            };
            let config = RegisterConfig::new(
                p2p_state.p2p_key,
                p2p_state.rendezvous_point_address,
                rendezvous_point,
                p2p_state.registration_ttl,
            );
            Ok(tokio::spawn(run_register_node(config, cancellation_token)))
        }
    }
}
