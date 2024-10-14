//! A CLI application that facilitates management operations over a running full node and other components.
#![warn(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]

mod db;
mod rpc;
mod storage;

use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use polka_storage_provider_common::rpc::ServerInfo;
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
use tokio::task::JoinError;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use crate::{
    db::{DBError, DealDB},
    rpc::{start_rpc_server, RpcServerState},
    storage::{start_upload_server, StorageServerState},
};

#[tokio::main]
async fn main() -> Result<(), ServerError> {
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
    Server::parse().run().await
}

/// Default parachain node adress.
const DEFAULT_NODE_ADDRESS: &str = "ws://127.0.0.1:42069";

/// Default address to bind the RPC server to.
pub(crate) const DEFAULT_RPC_LISTEN_ADDRESS: &str = "127.0.0.1:8000";

/// Default address to bind the RPC server to.
const DEFAULT_UPLOAD_LISTEN_ADDRESS: &str = "127.0.0.1:8001";

/// Retry interval to connect to the parachain RPC.
const RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// Number of retries to connect to the parachain RPC.
const RETRY_NUMBER: u32 = 5;

/// CLI components error handling implementor.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("FromEnv error: {0}")]
    EnvFilter(#[from] tracing_subscriber::filter::FromEnvError),

    #[error("URL parse error: {0}")]
    ParseUrl(#[from] url::ParseError),

    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),

    #[error("Error occurred while working with a car file: {0}")]
    Mater(#[from] mater::Error),

    #[error("no signer keypair was passed")]
    MissingKeypair,

    #[error("storage provider is not registered")]
    UnregisteredStorageProvider,

    #[error(transparent)]
    Subxt(#[from] subxt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Db(#[from] DBError),

    #[error(transparent)]
    Join(#[from] JoinError),
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Server {
    /// The server's listen address.
    #[arg(long, default_value = DEFAULT_UPLOAD_LISTEN_ADDRESS)]
    upload_listen_address: SocketAddr,

    /// The server's listen address.
    #[arg(long, default_value = DEFAULT_RPC_LISTEN_ADDRESS)]
    rpc_listen_address: SocketAddr,

    /// The target parachain node's address.
    #[arg(long, default_value = DEFAULT_NODE_ADDRESS)]
    node_address: Url,

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
    #[arg(long)]
    database_directory: Option<PathBuf>,

    /// Piece storage directory.
    #[arg(long)]
    storage_directory: Option<PathBuf>,
}

impl Server {
    pub async fn run(self) -> Result<(), ServerError> {
        let common_folder = PathBuf::new().join("/tmp").join(
            rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(7)
                .map(char::from)
                .collect::<String>(),
        );
        let database_dir = match self.database_directory {
            Some(database_dir) => database_dir,
            None => {
                let path = common_folder.join("deals_database");
                tracing::warn!(
                    "no database directory was defined, using a temporary location: {}",
                    path.display()
                );
                path
            }
        };
        tracing::debug!("database directory: {}", database_dir.display());

        let storage_dir = Arc::new(match self.storage_directory {
            Some(storage_dir) => storage_dir,
            None => {
                let path = common_folder.join("deals_storage");
                tracing::warn!(
                    "no storage directory was defined, using a temporary location: {}",
                    path.display()
                );
                path
            }
        });
        tracing::debug!("storage directory: {}", storage_dir.display());

        let Some(xt_keypair) = MultiPairSigner::new(
            self.sr25519_key.map(DebugPair::<Sr25519Pair>::into_inner),
            self.ecdsa_key.map(DebugPair::<ECDSAPair>::into_inner),
            self.ed25519_key.map(DebugPair::<Ed25519Pair>::into_inner),
        ) else {
            return Err(ServerError::MissingKeypair);
        };

        let xt_client =
            storagext::Client::new(self.node_address, RETRY_NUMBER, RETRY_INTERVAL).await?;

        // Check if the storage provider has been registered to the chain
        if let None = xt_client
            .retrieve_storage_provider(&subxt::utils::AccountId32(
                // account_id() -> sp_core::crypto::AccountId
                // as_ref() -> &[u8]
                // * -> [u8]
                *xt_keypair.account_id().as_ref(),
            ))
            .await?
        {
            tracing::warn!(concat!(
                "the provider key did not match a registered account id, ",
                "you can register your account using the ",
                "`storagext-cli storage-provider register`"
            ));
            return Err(ServerError::UnregisteredStorageProvider);
        }

        let deal_db = Arc::new(DealDB::new(database_dir)?);

        let upload_state = StorageServerState {
            storage_dir: storage_dir.clone(),
            deal_db: deal_db.clone(),
        };

        let rpc_state = RpcServerState {
            server_info: ServerInfo::new(xt_keypair.account_id()),
            storage_dir,
            deal_db,
            xt_client,
            xt_keypair,
        };

        let cancellation_token = tokio_util::sync::CancellationToken::new();

        // Start the servers
        let upload_task = tokio::spawn(start_upload_server(
            Arc::new(upload_state),
            self.upload_listen_address,
            cancellation_token.child_token(),
        ));
        let rpc_task = tokio::spawn(start_rpc_server(
            rpc_state,
            self.rpc_listen_address,
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
        let (upload_result, rpc_task) = tokio::join!(upload_task, rpc_task);

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

        // Exit with error
        upload_result??;
        rpc_task??;

        Ok(())
    }
}
