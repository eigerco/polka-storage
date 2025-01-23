use std::{fmt::Display, path::PathBuf, str::FromStr};

use bootstrap::bootstrap;
use clap::ValueEnum;
use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use libp2p::{identity::Keypair, rendezvous::Namespace, Multiaddr, PeerId};
use register::register;
use serde::{de, Deserialize};
use tokio_util::sync::CancellationToken;

mod bootstrap;
mod register;

pub(crate) use bootstrap::BootstrapConfig;
pub(crate) use register::RegisterConfig;

const P2P_NAMESPACE: &str = "polka-storage";

#[derive(Default, Debug, Clone, Copy, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    #[default]
    Bootstrap,
    Register,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Bootstrap => write!(f, "bootstrap"),
            NodeType::Register => write!(f, "register"),
        }
    }
}

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
    TOMLError(#[from] toml::de::Error),
    #[error("Failed to register at rendezvous point {0}")]
    RegistrationFailed(PeerId),
    #[error(transparent)]
    P2PTransport(#[from] libp2p::TransportError<std::io::Error>),
}

/// State struct for P2P node.
/// Holds all the information needed for spawning a node.
/// Node can be either a bootstrap or a registration node.
pub(crate) struct P2PState {
    /// P2P Node type, bootstrap or registration
    pub(crate) node_type: NodeType,

    /// P2P ED25519 private key
    pub(crate) p2p_key: Keypair,

    /// Rendezvous point address that the registration node connects to
    /// or the bootstrap node binds to.
    pub(crate) rendezvous_point_address: Multiaddr,

    /// PeerID of the bootstrap node used by the registration node.
    /// Optional because it is not used by the bootstrap node.
    pub(crate) rendezvous_point: Option<PeerId>,

    /// TTL of the p2p registration in seconds
    pub(crate) registration_ttl: u64,
}

/// Deserializes a ED25519 private key into a Keypair.
/// Can either be the private key as a string or the path of a PEM file with an @ prefixed
/// Calls `keypair_value_parser` after deserializing the source string
pub(crate) fn deser_keypair<'de, D: de::Deserializer<'de>>(d: D) -> Result<Keypair, D::Error> {
    let src: String = de::Deserialize::deserialize(d)?;
    keypair_value_parser(&src).map_err(de::Error::custom)
}

/// Parses a ED25519 private key into a Keypair.
/// Takes in a private key or the path to a PEM file, depending on the @ prefix.
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

/// Parses a string to an optional Peer ID.
/// Used in the [`ConfigurationArgs`] rendezvous_point field.
pub(crate) fn string_to_peer_id_option<'de, D: de::Deserializer<'de>>(
    d: D,
) -> Result<Option<PeerId>, D::Error> {
    let s: Option<String> = de::Deserialize::deserialize(d)?;
    match s {
        Some(s) => Ok(Some(PeerId::from_str(&s).map_err(de::Error::custom)?)),
        None => Ok(None),
    }
}

/// Runs a bootstrap node from the given config.
/// The `CancellationToken` is used for a graceful shutdown if the user presses ctrl+c
pub async fn run_bootstrap_node(
    config: BootstrapConfig,
    token: CancellationToken,
) -> Result<(), P2PError> {
    tracing::info!("Starting P2P bootstrap node");
    let (swarm, addr) = config.create_swarm()?;

    tokio::select! {
        res = bootstrap(swarm, addr) => {
            if let Err(e) = res {
                tracing::error!("Failed to start P2P node. Reason: {e}");
                return Err(e);
            }
        },
        _ = token.cancelled() => {
            tracing::info!("P2P node has been stopped by the cancellation token...");
            tracker.close();
            tracker.wait().await;
        },
    }

    Ok(())
}

/// Runs a registration node from the given config.
/// The `CancellationToken` is used for a graceful shutdown if the user presses ctrl+c
pub async fn run_register_node(
    config: RegisterConfig,
    token: CancellationToken,
) -> Result<(), P2PError> {
    tracing::info!("Starting P2P register node");
    let rendezvous_point = config.rendezvous_point;
    let rendezvous_point_address = config.rendezvous_point_address.clone();
    let registration_ttl = config.registration_ttl;
    let mut swarm = config.create_swarm()?;

    tokio::select! {
        res = register(
            &mut swarm,
            rendezvous_point,
            rendezvous_point_address,
            registration_ttl,
            Namespace::from_static(P2P_NAMESPACE),
        ) => {
            if let Err(e) = res {
                tracing::error!("Failed to start P2P node. Reason: {e}");
                return Err(e);
            }
        },
        _ = token.cancelled() => {
            tracing::info!("P2P node has been stopped by the cancellation token...");
            tracker.close();
            tracker.wait().await;
        },
    }

    Ok(())
}
