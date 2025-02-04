use bootstrap::bootstrap;
use log::info;

mod bootstrap;

pub(crate) use bootstrap::BootstrapConfig;

const DEFAULT_REGISTRATION_TTL: u64 = 86400;

#[derive(Debug, thiserror::Error)]
pub enum P2PError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Dial(#[from] libp2p::swarm::DialError),
    #[error("Invalid TCP config for swarm")]
    InvalidTcpConfig,
    #[error("Invalid behaviour config for swarm")]
    InvalidBehaviourConfig,
    #[error(transparent)]
    P2PTransport(#[from] libp2p::TransportError<std::io::Error>),
}

/// Runs a bootstrap node from the given config.
/// The `CancellationToken` is used for a graceful shutdown if the user presses ctrl+c
pub async fn run_bootstrap_node(config: BootstrapConfig) {
    info!("Starting P2P bootstrap node");
    let (swarm, addr) = config.create_swarm().expect("Could not create swarm");

    bootstrap(swarm, addr)
        .await
        .expect("Could not run bootstrap node");
}
