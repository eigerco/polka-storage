use hex::FromHexError;

// PoRep verifying key
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VerifyingKey {
    key: Vec<u8>,
}

impl VerifyingKey {
    pub fn from_raw_bytes(key: Vec<u8>) -> Self {
        Self { key }
    }

    /// Create a new verifying key from a hex string.
    pub fn from_hex(hex: &str) -> Result<Self, FromHexError> {
        let key = hex::decode(hex)?;

        Ok(Self { key })
    }
}

impl From<VerifyingKey> for Vec<u8> {
    fn from(value: VerifyingKey) -> Self {
        value.key
    }
}

#[cfg(feature = "clap")]
mod clap {
    use anyhow::anyhow;

    use super::VerifyingKey;

    impl VerifyingKey {
        /// `clap`'s custom parsing function.
        pub fn value_parser(src: &str) -> Result<Self, anyhow::Error> {
            use std::{path::PathBuf, str::FromStr};

            if let Some(stripped) = src.strip_prefix('@') {
                let path = PathBuf::from_str(stripped)
                    .expect("infallible")
                    .canonicalize()?;
                let key = std::fs::read(path)?;

                Ok(VerifyingKey::from_raw_bytes(key))
            } else {
                VerifyingKey::from_hex(src)
                    .map_err(|err| anyhow!("failed to parse key from hex: {}", err))
            }
        }
    }
}
