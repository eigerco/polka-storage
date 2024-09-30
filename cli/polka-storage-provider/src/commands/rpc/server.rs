use std::{net::SocketAddr, time::Duration};


use storagext::multipair::{DebugPair, MultiPairSigner};
use subxt::{
    ext::sp_core::{
        ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
    },
    tx::Signer,
};
use tokio::{signal, sync::oneshot};
use url::Url;

use crate::{
    commands::rpc::{DEFAULT_LISTEN_ADDRESS, DEFAULT_NODE_ADDRESS},
    rpc::server::{start_rpc_server, RpcServerState, ServerInfo},
};

/// Wait time for a graceful shutdown.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Retry interval to connect to the parachain RPC.
const RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// Number of retries to connect to the parachain RPC.
const RETRY_NUMBER: u32 = 5;

#[derive(Debug, thiserror::Error)]
pub enum ServerCommandError {
    #[error("no signer keypair was passed")]
    MissingKeypair,

    #[error(transparent)]
    Subxt(#[from] subxt::Error),
}

#[derive(Debug, clap::Parser)]
pub struct ServerCommand {
    /// The server's listen address.
    #[arg(long, default_value = DEFAULT_LISTEN_ADDRESS)]
    listen_address: SocketAddr,

    /// The target parachain node's address.
    #[arg(long, default_value = DEFAULT_NODE_ADDRESS)]
    node_address: Url,

    /// Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long,  value_parser = DebugPair::<Sr25519Pair>::value_parser)]
    sr25519_key: Option<DebugPair<Sr25519Pair>>,

    /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long,  value_parser = DebugPair::<ECDSAPair>::value_parser)]
    ecdsa_key: Option<DebugPair<ECDSAPair>>,

    /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long,  value_parser = DebugPair::<Ed25519Pair>::value_parser)]
    ed25519_key: Option<DebugPair<Ed25519Pair>>,
}

impl ServerCommand {
    pub async fn run(self) -> Result<(), ServerCommandError> {
        // NOTE: maybe try to register / check if the provider is registered?
        // once we try to run the server, we need to make sure its actually registered
        // otherwise, all interactions will just return something akin to (or just it)
        // "storage provider not registered"

        let Some(xt_keypair) = MultiPairSigner::new(
            self.sr25519_key.map(DebugPair::<Sr25519Pair>::into_inner),
            self.ecdsa_key.map(DebugPair::<ECDSAPair>::into_inner),
            self.ed25519_key.map(DebugPair::<Ed25519Pair>::into_inner),
        ) else {
            return Err(ServerCommandError::MissingKeypair);
        };

        let xt_client =
            storagext::Client::new(self.node_address.as_str(), RETRY_NUMBER, RETRY_INTERVAL)
                .await?;

        let state = RpcServerState {
            server_info: ServerInfo::new(xt_keypair.account_id()),
            xt_client,
            xt_keypair,
        };

        // Setup shutdown channel
        let (notify_shutdown_tx, notify_shutdown_rx) = oneshot::channel();

        // Start the server in the background
        let rpc_handler = tokio::spawn(start_rpc_server(
            state,
            self.listen_address,
            notify_shutdown_rx,
        ));

        // Wait for SIGTERM on the main thread and once received "unblock"
        signal::ctrl_c().await.expect("failed to listen for event");
        // Send the shutdown signal
        let _ = notify_shutdown_tx.send(());

        // Give server some time to finish
        tracing::info!("shutting down server, killing it in 10sec");
        let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, rpc_handler).await;

        tracing::info!("storage provider stopped");

        Ok(())
    }
}
