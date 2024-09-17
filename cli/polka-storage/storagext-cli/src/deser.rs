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

#[cfg(test)]
mod test {
    //! These tests basically ensure that the underlying parsers aren't broken without warning.

    use subxt::ext::sp_core::{
        ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
    };

    use crate::pair::DebugPair;

    #[track_caller]
    fn assert_debug_pair<P>(s: &str)
    where
        P: subxt::ext::sp_core::Pair,
    {
        let result_pair = DebugPair::<P>::value_parser(s).unwrap();
        let expect_pair = P::from_string(s, None).unwrap();

        assert_eq!(result_pair.0.to_raw_vec(), expect_pair.to_raw_vec());
    }

    #[test]
    fn deserialize_debug_pair_sr25519() {
        assert_debug_pair::<Sr25519Pair>("//Alice");
        // https://docs.substrate.io/reference/glossary/#dev-phrase
        // link visited on 23/7/2024 (you never know when Substrate's docs will become stale)
        assert_debug_pair::<Sr25519Pair>(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        );
        // secret seed for testing purposes
        assert_debug_pair::<Sr25519Pair>(
            "0xd045270857659c84705fbb367fd9644e5ab9b0c668f37c0bf28c6e72a120dd1f",
        );
    }

    #[test]
    fn deserialize_debug_pair_ecdsa() {
        assert_debug_pair::<ECDSAPair>("//Alice");
        // https://docs.substrate.io/reference/glossary/#dev-phrase
        // link visited on 23/7/2024 (you never know when Substrate's docs will become stale)
        assert_debug_pair::<ECDSAPair>(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        );
        // secret seed for testing purposes
        assert_debug_pair::<ECDSAPair>(
            "0xd045270857659c84705fbb367fd9644e5ab9b0c668f37c0bf28c6e72a120dd1f",
        );
    }

    #[test]
    fn deserialize_debug_pair_ed25519() {
        assert_debug_pair::<Ed25519Pair>("//Alice");
        // https://docs.substrate.io/reference/glossary/#dev-phrase
        // link visited on 23/7/2024 (you never know when Substrate's docs will become stale)
        assert_debug_pair::<Ed25519Pair>(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        );
        // secret seed for testing purposes
        assert_debug_pair::<Ed25519Pair>(
            "0xd045270857659c84705fbb367fd9644e5ab9b0c668f37c0bf28c6e72a120dd1f",
        );
    }
}
