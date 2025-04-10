use bootstrap::bootstrap;
use log::info;

mod bootstrap;

pub(crate) use bootstrap::BootstrapConfig;

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

/// Runs a bootstrap node from the given config.
/// The `CancellationToken` is used for a graceful shutdown if the user presses ctrl+c
pub async fn run_bootstrap_node(config: BootstrapConfig) {
    let (swarm, tcp_addr, ws_addr, bootstrap_addresses) = config
        .create_swarm()
        .await
        .expect("Could not create bootstrap swarm");

    info!(
        "Starting P2P bootstrap node with PeerID: {}",
        swarm.local_peer_id()
    );
    bootstrap(swarm, tcp_addr, ws_addr, bootstrap_addresses)
        .await
        .expect("Could not run bootstrap node");
}
