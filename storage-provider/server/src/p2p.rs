use std::{
    fmt::Display,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use bootstrap::{BootstrapBehaviour, BootstrapBehaviourEvent, BootstrapConfig};
use clap::ValueEnum;
use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use libp2p::{
    futures::StreamExt, identify, identity::Keypair, rendezvous, rendezvous::Namespace,
    swarm::SwarmEvent, Multiaddr, PeerId, Swarm,
};
use register::{RegisterBehaviour, RegisterBehaviourEvent, RegisterConfig};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{error, info};

mod bootstrap;
mod register;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum NodeType {
    Bootstrap,
    Register,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum P2PError {
    #[error(transparent)]
    SigningKeyError(#[from] ed25519_dalek::pkcs8::Error),
    #[error(transparent)]
    DecodingError(#[from] libp2p::identity::DecodingError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    DialError(#[from] libp2p::swarm::DialError),
    #[error("Invalid TCP config for swarm")]
    InvalidTcpConfig,
    #[error("Invalid behaviour config for swarm")]
    InvalidBehaviourConfig,
    #[error(transparent)]
    TOMLError(#[from] toml::de::Error),
    #[error("Failed to register at rendezvous point {0}")]
    RegistrationFailed(PeerId),
    #[error(transparent)]
    P2PTransportError(#[from] libp2p::TransportError<std::io::Error>),
}

fn create_keypair<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<Keypair, P2PError> {
    info!("Creating keypair from pem file at {path:?}");
    let key = SigningKey::read_pkcs8_pem_file(path)?;
    let keypair = Keypair::ed25519_from_bytes(key.to_bytes())?;

    Ok(keypair)
}

pub async fn start_p2p_node(
    node_type: NodeType,
    config: PathBuf,
    token: CancellationToken,
) -> Result<(), P2PError> {
    info!("Starting P2P node");
    let tracker = TaskTracker::new();

    tokio::select! {
        res = run_p2p_node(node_type, config) => {
            if let Err(e) = res {
                error!("Failed to start P2P node. Reason: {e}");
                return Err(e);
            }
        },
        _ = token.cancelled() => {
            tracing::info!("P2P node has been stopped by the cancellation token...");
        },
    }

    tracker.close();
    tracker.wait().await;

    Ok(())
}

async fn run_p2p_node(node_type: NodeType, config: PathBuf) -> Result<(), P2PError> {
    match node_type {
        NodeType::Bootstrap => {
            let contents = read_to_string(config)?;
            let config: BootstrapConfig = toml::from_str(&contents)?;
            let (swarm, addr) = config.create_swarm()?;

            bootstrap(swarm, addr).await
        }
        NodeType::Register => {
            let contents = read_to_string(config)?;
            let config: RegisterConfig = toml::from_str(&contents)?;
            let (swarm, rendezvous_point_address, rendezvous_point) = config.create_swarm()?;

            register(
                swarm,
                rendezvous_point,
                rendezvous_point_address,
                None,
                Namespace::from_static("rendezvous"),
            )
            .await
        }
    }
}

/// Register the peer with the rendezvous point.
/// The ttl is how long the peer will remain registered in seconds.
async fn register(
    mut swarm: Swarm<RegisterBehaviour>,
    rendezvous_point: PeerId,
    rendezvous_point_address: Multiaddr,
    ttl: Option<u64>,
    namespace: Namespace,
) -> Result<(), P2PError> {
    info!("Attempting to register with rendezvous point {rendezvous_point} at {rendezvous_point_address}");
    swarm.dial(rendezvous_point_address.clone())?;

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {}", address);
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                cause: Some(error),
                ..
            } if peer_id == rendezvous_point => {
                info!("Lost connection to rendezvous point {}", error);
            }
            // once `/identify` did its job, we know our external address and can register
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Identify(
                identify::Event::Received { info, .. },
            )) => {
                // Register our external address.
                info!("Registering external address {}", info.observed_addr);
                swarm.add_external_address(info.observed_addr);
                if let Err(error) = swarm.behaviour_mut().rendezvous.register(
                    namespace.clone(),
                    rendezvous_point,
                    ttl,
                ) {
                    error!("Failed to register: {error}");
                    return Err(P2PError::RegistrationFailed(rendezvous_point));
                }
            }
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Rendezvous(
                rendezvous::client::Event::Registered {
                    namespace,
                    ttl,
                    rendezvous_node,
                },
            )) => {
                info!(
                    "Registered for namespace '{}' at rendezvous point {} for the next {} seconds",
                    namespace, rendezvous_node, ttl
                );
                return Ok(());
            }
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Rendezvous(
                rendezvous::client::Event::RegisterFailed {
                    rendezvous_node,
                    namespace,
                    error,
                },
            )) => {
                error!(
                    "Failed to register: rendezvous_node={}, namespace={}, error_code={:?}",
                    rendezvous_node, namespace, error
                );
                return Err(P2PError::RegistrationFailed(rendezvous_node));
            }
            _other => {}
        }
    }

    Ok(())
}

/// Run the rendezvous point (bootstrap node).
/// Listens on the given [`Multiaddr`]
async fn bootstrap(mut swarm: Swarm<BootstrapBehaviour>, addr: Multiaddr) -> Result<(), P2PError> {
    info!("Starting P2P bootstrap node at {addr}");
    swarm.listen_on(addr)?;
    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Connected to {}", peer_id);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Disconnected from {}", peer_id);
            }
            SwarmEvent::Behaviour(BootstrapBehaviourEvent::Rendezvous(
                rendezvous::server::Event::PeerRegistered { peer, registration },
            )) => {
                info!(
                    "Peer {} registered for namespace '{}' for {} seconds",
                    peer, registration.namespace, registration.ttl
                );
            }
            SwarmEvent::Behaviour(BootstrapBehaviourEvent::Rendezvous(
                rendezvous::server::Event::DiscoverServed {
                    enquirer,
                    registrations,
                },
            )) => {
                if !registrations.is_empty() {
                    info!(
                        "Served peer {} with {} new registrations",
                        enquirer,
                        registrations.len()
                    );
                }
            }
            _other => {}
        }
    }
    Ok(())
}
