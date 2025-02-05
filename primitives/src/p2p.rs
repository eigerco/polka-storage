use std::{path::PathBuf, str::FromStr};

use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use libp2p::identity::Keypair;

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
