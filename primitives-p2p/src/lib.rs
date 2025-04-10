mod request_response;

use std::{path::PathBuf, str::FromStr};

use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use libp2p::{identity::Keypair, Multiaddr};

// These are exports and I don't want them mixed up with normal imports.
#[rustfmt::skip]
pub use request_response::{
    bootstrap::{PeerIdRequest, PeerInfo, PeerInfoResponse, BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL},
    storage_provider::{
        PieceInfo, PieceInfoRequest, PieceInfoResponse, SP_REQUEST_RESPONSE_PROTOCOL,
    },
};

pub use request_response::services;

pub const GOSSIP_TOPIC: &str = "registrar";
pub const IDENTIFY_PROTOCOL_VERSION: &str = "polka-storage/1.0.0";
pub const DEFAULT_REGISTRATION_TTL: u64 = 86400;

pub fn validate_tcp_multiaddr(s: &str) -> Result<Multiaddr, String> {
    const IP4_TCP: [&str; 2] = ["ip4", "tcp"];
    const IP6_TCP: [&str; 2] = ["ip6", "tcp"];

    let multiaddress = Multiaddr::from_str(s).map_err(|err| err.to_string())?;
    let protocols = multiaddress.protocol_stack().collect::<Vec<_>>();
    if protocols.is_empty() {
        // Not sure if this is even possible, but checking doesn't hurt
        return Err(format!("No protocols were detected for {s}"));
    }
    // ip6 isn't tested but just like above, it doesn't hurt to check
    if protocols.as_slice() != IP4_TCP && protocols.as_slice() != IP6_TCP {
        return Err(format!("Unsupported protocol stack: {:?}", protocols));
    }
    Ok(multiaddress)
}

pub fn validate_ws_multiaddr(s: &str) -> Result<Multiaddr, String> {
    const IP4_TCP_WS: [&str; 3] = ["ip4", "tcp", "ws"];
    const IP6_TCP_WS: [&str; 3] = ["ip6", "tcp", "ws"];

    let multiaddress = Multiaddr::from_str(s).map_err(|err| err.to_string())?;
    let protocols = multiaddress.protocol_stack().collect::<Vec<_>>();
    if protocols.is_empty() {
        // Not sure if this is even possible, but checking doesn't hurt
        return Err(format!("No protocols were detected for {s}"));
    }
    // ip6 isn't tested but just like above, it doesn't hurt to check
    if protocols.as_slice() != IP4_TCP_WS && protocols.as_slice() != IP6_TCP_WS {
        return Err(format!("Unsupported protocol stack: {:?}", protocols));
    }
    Ok(multiaddress)
}

/// Parses a ED25519 private key into a Keypair.
/// Takes in a private key or the path to a PEM file, depending on the @ prefix.
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
