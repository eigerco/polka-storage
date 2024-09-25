//! Types in this module are defined to enable deserializing them from the CLI arguments or similar.
use std::{path::PathBuf, str::FromStr};

/// A type that can take a string representing either a path to the serialized type or
/// the full content for deserialization.
pub trait DeserializablePath: serde::de::DeserializeOwned {
    /// Parse a `&str` as a path if it starts with an `@` symbol, or as JSON otherwise.
    fn deserialize_json(src: &str) -> Result<Self, anyhow::Error> {
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

impl<T> DeserializablePath for T where T: serde::de::DeserializeOwned {}
