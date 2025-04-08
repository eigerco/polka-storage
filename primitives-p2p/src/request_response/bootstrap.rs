use libp2p::{Multiaddr, PeerId};

pub const BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL: &str = "/polka-storage-bootstrap-req-resp/1.0.0";

/// Struct holds peer information.
/// - PeerId: Registered Peer ID.
/// - multiaddrs: Vec of multiaddresses the peer has registered.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct PeerInfo {
    #[cfg_attr(
        feature = "serde",
        serde(
            serialize_with = "serialize_peer_id",
            deserialize_with = "deserialize_peer_id"
        )
    )]
    pub peer_id: PeerId,
    pub multiaddrs: Vec<Multiaddr>,
}

/// This enum is used in the request response P2P protocol.
/// PeerInfoResponse::NotFound is returned when the requested peer ID was not found.
/// PeerInfoResponse::Found(..) is returned when the requested peer ID was found.
/// The latter holds the relevant [`PeerInfo`] inside.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub enum PeerInfoResponse {
    Found(PeerInfo),
    NotFound(PeerIdRequest),
}

/// The request type used for the request response P2P protocol.
/// We cannot use PeerId directly because it does not implement
/// Serialize and Deserialize.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct PeerIdRequest(
    #[cfg_attr(
        feature = "serde",
        serde(
            serialize_with = "serialize_peer_id",
            deserialize_with = "deserialize_peer_id"
        )
    )]
    pub PeerId,
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

#[cfg(feature = "serde")]
use _serde::{deserialize_peer_id, serialize_peer_id};

#[cfg(feature = "serde")]
mod _serde {
    use std::str::FromStr;

    use libp2p::PeerId;
    use serde::{de, Serializer};

    pub(super) fn deserialize_peer_id<'de, D: de::Deserializer<'de>>(
        d: D,
    ) -> Result<PeerId, D::Error> {
        let s: String = de::Deserialize::deserialize(d)?;
        PeerId::from_str(&s).map_err(de::Error::custom)
    }

    pub(super) fn serialize_peer_id<S: Serializer>(
        id: &PeerId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let id = id.to_string();
        serializer.collect_str(&id)
    }
}
