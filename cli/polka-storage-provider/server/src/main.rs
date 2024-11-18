//! A CLI application that facilitates management operations over a running full node and other components.
#![warn(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]

mod db;
mod pipeline;
mod rpc;
mod storage;

use std::{env::temp_dir, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use pipeline::types::PipelineMessage;
use polka_storage_proofs::porep::{self, PoRepParameters};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives_proofs::{RegisteredPoStProof, RegisteredSealProof};
use rand::Rng;
use storagext::{
    multipair::{DebugPair, MultiPairSigner},
    StorageProviderClientExt,
};
use subxt::{
    ext::sp_core::{
        ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
    },
    tx::Signer,
};
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinError};
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use crate::{
    db::{DBError, DealDB},
    pipeline::{start_pipeline, PipelineState},
    rpc::{start_rpc_server, RpcServerState},
    storage::{start_upload_server, StorageServerState},
};

/// Default address to bind the RPC server to.
pub(crate) const DEFAULT_RPC_LISTEN_ADDRESS: &str = "127.0.0.1:8000";

/// Default parachain node adress.
const DEFAULT_NODE_ADDRESS: &str = "ws://127.0.0.1:42069";

/// Default address to bind the RPC server to.
const DEFAULT_UPLOAD_LISTEN_ADDRESS: &str = "127.0.0.1:8001";

/// Retry interval to connect to the parachain RPC.
const RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// Number of retries to connect to the parachain RPC.
const RETRY_NUMBER: u32 = 5;

/// Name for the directory where the CAR wrapped pieces are kept.
const CAR_PIECE_DIRECTORY_NAME: &str = "car";

/// Name for the directory where the unsealed pieces are kept.
const UNSEALED_SECTOR_DIRECTORY_NAME: &str = "unsealed";

/// Name for the directory where the sealed pieces are kept.
const SEALED_SECTOR_DIRECTORY_NAME: &str = "sealed";

/// Name for the directory where the sealing cache is kept.
const SEALING_CACHE_DIRECTORY_NANE: &str = "cache";

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
}
fn main() -> Result<(), ServerError> {
    // Logger initialization.
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .init();

    // Run requested command.
    let configuration: ServerConfiguration = ServerArguments::parse().try_into()?;

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

    #[error("registered proof does not match the configuration")]
    ProofMismatch,

    #[error("proof sectors sizes do not match")]
    SectorSizeMismatch,

    #[error("failed to load PoRep parameters from: {0}, because: {1}")]
    InvalidPoRepParameters(std::path::PathBuf, porep::PoRepError),

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
}

/// The server arguments, as passed by the user, unvalidated.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct ServerArguments {
    /// The server's listen address.
    #[arg(long, default_value = DEFAULT_UPLOAD_LISTEN_ADDRESS)]
    upload_listen_address: SocketAddr,

    /// The server's listen address.
    #[arg(long, default_value = DEFAULT_RPC_LISTEN_ADDRESS)]
    rpc_listen_address: SocketAddr,

    /// The target parachain node's address.
    #[arg(long, default_value = DEFAULT_NODE_ADDRESS)]
    node_url: Url,

    /// Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, value_parser = DebugPair::<Sr25519Pair>::value_parser)]
    sr25519_key: Option<DebugPair<Sr25519Pair>>,

    /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, value_parser = DebugPair::<ECDSAPair>::value_parser)]
    ecdsa_key: Option<DebugPair<ECDSAPair>>,

    /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, value_parser = DebugPair::<Ed25519Pair>::value_parser)]
    ed25519_key: Option<DebugPair<Ed25519Pair>>,

    /// RocksDB storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/deals_database`.
    #[arg(long)]
    database_directory: Option<PathBuf>,

    /// Piece storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/...`.
    #[arg(long)]
    storage_directory: Option<PathBuf>,

    /// Proof of Replication proof type.
    #[arg(long)]
    seal_proof: RegisteredSealProof,

    /// Proof of Spacetime proof type.
    #[arg(long)]
    post_proof: RegisteredPoStProof,

    /// Proving Parameters for PoRep proof, corresponding to given `seal_proof` sector size.
    /// They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.
    /// Shared parameters available to get in the [root repo](http://github.com/eigerco/polka-storage/README.md#Parameters).
    ///
    /// Testing/temporary parameters can be also generated via `polka-storage-provider-client proofs porep-params` command.
    /// Note that when you generate keys, for local testnet,
    /// **they need to be set** via an extrinsic pallet-proofs::set_porep_verifyingkey.
    #[arg(long)]
    porep_parameters: PathBuf,
}

/// A valid server configuration. To be created using [`ServerConfiguration::try_from`].
///
/// The main difference to [`Server`] is that this structure only contains validated and
/// ready to use parameters.
pub struct ServerConfiguration {
    /// Storage server listen address.
    upload_listen_address: SocketAddr,

    /// RPC server listen address.
    rpc_listen_address: SocketAddr,

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
}

impl TryFrom<ServerArguments> for ServerConfiguration {
    type Error = ServerError;

    fn try_from(value: ServerArguments) -> Result<Self, Self::Error> {
        if value.post_proof.sector_size() != value.seal_proof.sector_size() {
            return Err(ServerError::SectorSizeMismatch);
        }

        let multi_pair_signer = MultiPairSigner::new(
            value.sr25519_key.map(DebugPair::<Sr25519Pair>::into_inner),
            value.ecdsa_key.map(DebugPair::<ECDSAPair>::into_inner),
            value.ed25519_key.map(DebugPair::<Ed25519Pair>::into_inner),
        )
        .ok_or(ServerError::MissingKeypair)?;

        let common_folder = get_random_temporary_folder();
        let database_directory = value.database_directory.unwrap_or_else(|| {
            let path = common_folder.join("deals_database");
            tracing::warn!(
                "no database directory was defined, using: {}",
                path.display()
            );
            path
        });
        std::fs::create_dir_all(&database_directory)?;

        let storage_directory = value.storage_directory.unwrap_or_else(|| {
            let path = common_folder.join("deals_storage");
            tracing::warn!(
                "no storage directory was defined, using: {}",
                path.display()
            );
            path
        });
        std::fs::create_dir_all(&storage_directory)?;

        let porep_parameters = porep::load_groth16_parameters(value.porep_parameters.clone())
            .map_err(|e| ServerError::InvalidPoRepParameters(value.porep_parameters, e))?;

        Ok(Self {
            upload_listen_address: value.upload_listen_address,
            rpc_listen_address: value.rpc_listen_address,
            node_url: value.node_url,
            multi_pair_signer,
            database_directory,
            storage_directory,
            seal_proof: value.seal_proof,
            post_proof: value.post_proof,
            porep_parameters,
        })
    }
}

impl ServerConfiguration {
    pub async fn run(self) -> Result<(), ServerError> {
        let SetupOutput {
            storage_state,
            rpc_state,
            pipeline_state,
            pipeline_rx,
        } = self.setup().await?;

        let cancellation_token = CancellationToken::new();

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

        // Wait for SIGTERM on the main thread and once received "unblock"
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for event");
        tracing::info!("SIGTERM received, shutting down...");

        cancellation_token.cancel();
        tracing::info!("sent shutdown signal");

        // Wait for the tasks to finish
        let (upload_result, rpc_task, pipeline_task) =
            tokio::join!(storage_task, rpc_task, pipeline_task);

        // Log errors
        let upload_result = upload_result
            .inspect_err(|err| tracing::error!(%err))
            .inspect(|ok| {
                let _ = ok.as_ref().inspect_err(|err| tracing::error!(%err));
            });
        let rpc_task = rpc_task
            .inspect_err(|err| tracing::error!(%err))
            .inspect(|ok| {
                let _ = ok.as_ref().inspect_err(|err| tracing::error!(%err));
            });

        let pipeline_task = pipeline_task
            .inspect_err(|err| tracing::error!(%err))
            .inspect(|ok| {
                let _ = ok.as_ref().inspect_err(|err| tracing::error!(%err));
            });

        // Exit with error
        upload_result??;
        rpc_task??;
        pipeline_task??;

        Ok(())
    }

    async fn setup(self) -> Result<SetupOutput, ServerError> {
        let xt_client = Arc::new(
            ServerConfiguration::setup_storagext_client(
                self.node_url,
                &self.multi_pair_signer,
                &self.post_proof,
            )
            .await?,
        );
        let deal_database = Arc::new(DealDB::new(self.database_directory)?);

        // Car piece storage directory — i.e. the CAR archives from the input streams
        let car_piece_storage_dir = Arc::new(self.storage_directory.join(CAR_PIECE_DIRECTORY_NAME));
        let unsealed_sector_storage_dir =
            Arc::new(self.storage_directory.join(UNSEALED_SECTOR_DIRECTORY_NAME));
        let sealed_sector_storage_dir =
            Arc::new(self.storage_directory.join(SEALED_SECTOR_DIRECTORY_NAME));
        let sealing_cache_dir = Arc::new(self.storage_directory.join(SEALING_CACHE_DIRECTORY_NANE));

        // Create the storage directories
        tokio::fs::create_dir_all(car_piece_storage_dir.as_ref()).await?;
        tokio::fs::create_dir_all(unsealed_sector_storage_dir.as_ref()).await?;
        tokio::fs::create_dir_all(sealed_sector_storage_dir.as_ref()).await?;
        tokio::fs::create_dir_all(sealing_cache_dir.as_ref()).await?;

        let (pipeline_tx, pipeline_rx) = tokio::sync::mpsc::unbounded_channel::<PipelineMessage>();

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
            unsealed_sectors_dir: unsealed_sector_storage_dir,
            sealed_sectors_dir: sealed_sector_storage_dir,
            sealing_cache_dir,
            porep_parameters: Arc::new(self.porep_parameters),
            xt_client,
            xt_keypair: self.multi_pair_signer,
            pipeline_sender: pipeline_tx,
        };

        Ok(SetupOutput {
            storage_state,
            rpc_state,
            pipeline_state,
            pipeline_rx,
        })
    }

    async fn setup_storagext_client(
        rpc_address: impl AsRef<str>,
        xt_keypair: &MultiPairSigner,
        post_proof: &RegisteredPoStProof,
    ) -> Result<storagext::Client, ServerError> {
        let xt_client = storagext::Client::new(rpc_address, RETRY_NUMBER, RETRY_INTERVAL).await?;

        // Check if the storage provider has been registered to the chain
        let storage_provider_info = xt_client
            .retrieve_storage_provider(&subxt::utils::AccountId32(
                // account_id() -> sp_core::crypto::AccountId
                // as_ref() -> &[u8]
                // * -> [u8]
                *xt_keypair.account_id().as_ref(),
            ))
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
            }
            None => {
                tracing::error!(concat!(
                    "the provider key did not match a registered account id, ",
                    "you can register your account using the ",
                    "`storagext-cli storage-provider register`"
                ));
                return Err(ServerError::UnregisteredStorageProvider);
            }
        }

        Ok(xt_client)
    }
}
