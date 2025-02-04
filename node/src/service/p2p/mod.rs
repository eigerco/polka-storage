use std::str::FromStr;

use bootstrap::bootstrap;
use libp2p::{Multiaddr, PeerId};
use log::info;
use serde::{de, Deserialize, Serialize, Serializer};

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
    #[error(transparent)]
    P2PSubscription(#[from] libp2p::gossipsub::SubscriptionError),
}

/// Runs a bootstrap node from the given config.
/// The `CancellationToken` is used for a graceful shutdown if the user presses ctrl+c
pub async fn run_bootstrap_node(config: BootstrapConfig) {
    info!("Starting P2P bootstrap node");
    let (swarm, addr, bootstrap_addresses) = config
        .create_swarm()
        .expect("Could not create bootstrap swarm");

    bootstrap(swarm, addr, bootstrap_addresses)
        .await
        .expect("Could not run bootstrap node");
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    #[serde(serialize_with = "serialize_peer_id")]
    #[serde(deserialize_with = "deserialize_peer_id")]
    pub peer_id: PeerId,
    pub multiaddrs: Vec<Multiaddr>,
}

fn deserialize_peer_id<'de, D: de::Deserializer<'de>>(d: D) -> Result<PeerId, D::Error> {
    let s: String = de::Deserialize::deserialize(d)?;
    PeerId::from_str(&s).map_err(de::Error::custom)
}

fn serialize_peer_id<S: Serializer>(id: &PeerId, serializer: S) -> Result<S::Ok, S::Error> {
    let id = id.to_string();
    serializer.collect_str(&id)
}
