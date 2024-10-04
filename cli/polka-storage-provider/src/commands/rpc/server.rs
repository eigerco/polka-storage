use std::{cell::OnceCell, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

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
use url::Url;

use crate::{
    db::{DBError, DealDB},
    rpc::server::{start_rpc_server, RpcServerState, ServerInfo},
    storage::{start_upload_server, StorageServerState},
};

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

const RANDOM_ROOT: OnceCell<String> = OnceCell::new();

/// Get a "common" temporary folder.
///
/// This is, admitedly, a contrived way of reimplementing a tempdir mechanism, however,
/// tempdir works per instance (which we could probably resolve using the [`OnceCell`] trick too)
/// and deletes the folder when dropped, which is not helpful at all when running a test instance.
fn get_common_folder() -> PathBuf {
    let cell = RANDOM_ROOT;
    let root = cell.get_or_init(|| {
        rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(7)
            .map(char::from)
            .collect()
    });

    PathBuf::new().join("/tmp").join(root)
}

/// Get the default database directory — i.e. `/tmp/<random 7 characters>/deals_database`.
fn default_database_dir() -> PathBuf {
    get_common_folder().join("deals_database")
}

/// Get the default storage directory — i.e. `/tmp/<random 7 characters>/deals_storage`.
fn default_storage_dir() -> PathBuf {
    get_common_folder().join("deals_storage")
}

#[derive(Debug, thiserror::Error)]
pub enum ServerCommandError {
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

#[derive(Debug, clap::Parser)]
pub struct ServerCommand {
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
    #[arg(alias = "database_dir", long, default_value = default_database_dir().into_os_string())]
    database_directory: PathBuf,

    /// Piece storage directory.
    #[arg(alias = "storage_dir", long, default_value = default_storage_dir().into_os_string())]
    storage_directory: PathBuf,
}

impl ServerCommand {
    pub async fn run(self) -> Result<(), ServerCommandError> {
        let storage_dir = Arc::new(self.storage_directory);
        tracing::debug!("storage directory: {}", storage_dir.display());
        tracing::debug!("database directory: {}", self.database_directory.display());

        let Some(xt_keypair) = MultiPairSigner::new(
            self.sr25519_key.map(DebugPair::<Sr25519Pair>::into_inner),
            self.ecdsa_key.map(DebugPair::<ECDSAPair>::into_inner),
            self.ed25519_key.map(DebugPair::<Ed25519Pair>::into_inner),
        ) else {
            return Err(ServerCommandError::MissingKeypair);
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
            return Err(ServerCommandError::UnregisteredStorageProvider);
        }

        let deal_db = Arc::new(DealDB::new(self.database_directory)?);

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
