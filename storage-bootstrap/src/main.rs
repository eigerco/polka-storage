use bootstrap::bootstrap;

mod bootstrap;
mod query;
mod swarm;

pub(crate) use bootstrap::BootstrapConfig;
use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::query::QueryConfig;

#[derive(Debug, thiserror::Error)]
pub enum P2PError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Dial(#[from] libp2p::swarm::DialError),
    #[error("Invalid TCP config for swarm")]
    InvalidTcpConfig,
    #[error("Invalid Websocket config for swarm")]
    InvalidWebsocketConfig,
    #[error("Invalid behaviour config for swarm")]
    InvalidBehaviourConfig,
    #[error(transparent)]
    P2PTransport(#[from] libp2p::TransportError<std::io::Error>),
}

#[derive(Debug, Clone, clap::Parser)]
enum App {
    #[command()]
    Run(BootstrapConfig),

    #[command()]
    Query(QueryConfig),
}

/// Runs a bootstrap node from the given config.
/// The `CancellationToken` is used for a graceful shutdown if the user presses ctrl+c
pub async fn run_bootstrap_node(config: BootstrapConfig) -> Result<(), P2PError> {
    let (swarm, listen_addresses, public_addresses, bootstrap_addresses) = config
        .create_swarm()
        .await
        .expect("Could not create bootstrap swarm");

    tracing::info!(
        "Starting P2P bootstrap node with PeerID: {}",
        swarm.local_peer_id()
    );
    bootstrap(
        swarm,
        listen_addresses,
        public_addresses,
        bootstrap_addresses,
    )
    .await
}

fn main() {
    tracing_subscriber::registry()
        .with(
            fmt::layer().with_filter(
                EnvFilter::builder()
                    .with_default_directive(if cfg!(debug_assertions) {
                        LevelFilter::DEBUG.into()
                    } else {
                        LevelFilter::INFO.into()
                    })
                    .from_env()
                    .expect("able to read configuration from env"),
            ),
        )
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async {
            match App::parse() {
                App::Run(bootstrap) => run_bootstrap_node(bootstrap).await,
                App::Query(query) => query.run().await,
            }
        })
        .unwrap();
}
