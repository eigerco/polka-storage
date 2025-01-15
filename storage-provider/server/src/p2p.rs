use std::{fmt::Display, path::PathBuf, str::FromStr};

use bootstrap::{BootstrapBehaviour, BootstrapBehaviourEvent};
use clap::ValueEnum;
use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use libp2p::{
    futures::StreamExt, identify, identity::Keypair, rendezvous, rendezvous::Namespace,
    swarm::SwarmEvent, Multiaddr, PeerId, Swarm,
};
use register::{RegisterBehaviour, RegisterBehaviourEvent};
use serde::{de, Deserialize};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{error, info};

mod bootstrap;
mod register;

pub(crate) use bootstrap::BootstrapConfig;
pub(crate) use register::RegisterConfig;

const P2P_NAMESPACE: &str = "polka-storage";

#[derive(Default, Debug, Clone, Copy, ValueEnum, Deserialize)]
pub enum NodeType {
    #[default]
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

pub(crate) struct P2PState {
    /// P2P ED25519 private key
    pub(crate) p2p_key: Keypair,

    /// Rendezvous point address that the registration node connects to
    /// or the bootstrap node binds to.
    pub(crate) rendezvous_point_address: Multiaddr,

    /// PeerID of the bootstrap node used by the registration node.
    /// Optional because it is not used by the bootstrap node.
    pub(crate) rendezvous_point: Option<PeerId>,
}

pub(crate) fn deser_keypair<'de, D: de::Deserializer<'de>>(d: D) -> Result<Keypair, D::Error> {
    let src: String = de::Deserialize::deserialize(d)?;
    keypair_value_parser(&src).map_err(de::Error::custom)
}

pub(crate) fn keypair_value_parser(src: &str) -> Result<Keypair, String> {
    let key = if let Some(stripped) = src.strip_prefix('@') {
        let path = PathBuf::from_str(stripped)
            .map_err(|e| e.to_string())?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        SigningKey::read_pkcs8_pem_file(path).map_err(|e| e.to_string())?
    } else {
        let hex_key = hex::decode(src).map_err(|e| e.to_string())?;
        SigningKey::try_from(hex_key.as_slice()).map_err(|e| e.to_string())?
    };
    Keypair::ed25519_from_bytes(key.to_bytes()).map_err(|e| e.to_string())
}

pub(crate) fn string_to_peer_id_option<'de, D: de::Deserializer<'de>>(
    d: D,
) -> Result<Option<PeerId>, D::Error> {
    let s: Option<String> = de::Deserialize::deserialize(d)?;
    match s {
        Some(s) => Ok(Some(PeerId::from_str(&s).map_err(de::Error::custom)?)),
        None => Ok(None),
    }
}

pub async fn run_bootstrap_node(
    config: BootstrapConfig,
    token: CancellationToken,
) -> Result<(), P2PError> {
    info!("Starting P2P bootstrap node");
    let tracker = TaskTracker::new();
    let (swarm, addr) = config.create_swarm()?;

    tokio::select! {
        res = bootstrap(swarm, addr) => {
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

pub async fn run_register_node(
    config: RegisterConfig,
    token: CancellationToken,
) -> Result<(), P2PError> {
    info!("Starting P2P register node");
    let tracker = TaskTracker::new();
    let (swarm, rendezvous_point_address, rendezvous_point) = config.create_swarm()?;

    tokio::select! {
        res = register(
            swarm,
            rendezvous_point,
            rendezvous_point_address,
            None,
            Namespace::from_static(P2P_NAMESPACE),
        ) => {
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
