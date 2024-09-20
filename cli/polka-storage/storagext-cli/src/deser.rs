//! Types in this module are defined to enable deserializing them from the CLI arguments or similar.
use std::{fmt::Debug, path::PathBuf, str::FromStr};

use cid::Cid;
pub(crate) trait ParseablePath: serde::de::DeserializeOwned {
    fn parse_json(src: &str) -> Result<Self, anyhow::Error> {
        Ok(if let Some(stripped) = src.strip_prefix('@') {
            let path = PathBuf::from_str(stripped)?.canonicalize()?;
            let file = std::fs::File::open(path)?;
            let mut buffered_file = std::io::BufReader::new(file);
            serde_json::from_reader(&mut buffered_file)
        } else {
            serde_json::from_str(src)
        }?)
    }
}

impl<T> ParseablePath for T where T: serde::de::DeserializeOwned {}

/// CID doesn't deserialize from a string, hence we need our work wrapper.
///
/// <https://github.com/multiformats/rust-cid/issues/162>
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CidWrapper(pub(crate) Cid);

impl<'de> serde::de::Deserialize<'de> for CidWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let cid = Cid::try_from(s.as_str()).map_err(|e| {
            serde::de::Error::custom(format!(
                "failed to parse CID, check that the input is a valid CID: {e:?}"
            ))
        })?;
        Ok(Self(cid))
    }
}
