use std::{path::PathBuf, str::FromStr};

use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use serde::{de, Deserialize, Serialize, Serializer};

/// Parses a ED25519 private key into a Keypair.
/// Takes in a private key or the path to a PEM file, depending on the @ prefix.
#[cfg(feature = "std")]
pub fn keypair_value_parser(src: &str) -> Result<Keypair, String> {
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

/// Struct holds peer information.
/// - PeerId: Registered Peer ID.
/// - multiaddrs: Vec of multiaddresses the peer has registered.
#[cfg(feature = "std")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    #[serde(serialize_with = "serialize_peer_id")]
    #[serde(deserialize_with = "deserialize_peer_id")]
    pub peer_id: PeerId,
    pub multiaddrs: Vec<Multiaddr>,
}

/// This enum is used in the request response P2P protocol.
/// PeerInfoResponse::NotFound is returned when the requested peer ID was not found.
/// PeerInfoResponse::Found(..) is returned when the requested peer ID was found.
/// The latter holds the relevant [`PeerInfo`] inside.
#[cfg(feature = "std")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PeerInfoResponse {
    Found(PeerInfo),
    NotFound(PeerIdRequest),
}

/// The request type used for the request response P2P protocol.
/// We cannot use PeerId directly because it does not implement
/// Serialize and Deserialize.
#[cfg(feature = "std")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerIdRequest(
    #[serde(serialize_with = "serialize_peer_id")]
    #[serde(deserialize_with = "deserialize_peer_id")]
    PeerId,
);

impl From<PeerId> for PeerIdRequest {
    fn from(value: PeerId) -> Self {
        Self(value)
    }
}

impl Into<PeerId> for PeerIdRequest {
    fn into(self) -> PeerId {
        self.0
    }
}

fn deserialize_peer_id<'de, D: de::Deserializer<'de>>(d: D) -> Result<PeerId, D::Error> {
    let s: String = de::Deserialize::deserialize(d)?;
    PeerId::from_str(&s).map_err(de::Error::custom)
}

fn serialize_peer_id<S: Serializer>(id: &PeerId, serializer: S) -> Result<S::Ok, S::Error> {
    let id = id.to_string();
    serializer.collect_str(&id)
}
