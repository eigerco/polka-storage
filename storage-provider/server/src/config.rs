use std::{
    env::temp_dir,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZero,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use polka_storage_provider_common::config::sealing::SealingConfiguration;
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use rand::Rng;
use serde::Deserialize;
use url::Url;

use crate::ServerError;

/// Default address to bind the RPC server to.
const fn default_upload_listen_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001)
}

/// Default number of parallel prove commits.
const fn default_parallel_prove_commits() -> NonZero<usize> {
    // SAFETY: 2 != 0
    unsafe { NonZero::new_unchecked(2) }
}

/// Default parachain node adress.
const DEFAULT_NODE_ADDRESS: &str = "ws://127.0.0.1:42069";

fn default_node_address() -> Url {
    Url::parse(DEFAULT_NODE_ADDRESS).expect("DEFAULT_NODE_ADDRESS must be a valid Url")
}

static RANDOM_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Generates a random temporary directory.
///
/// This function will always yield the same random path for a *single program execution*.
fn default_random_directory() -> PathBuf {
    RANDOM_PATH
        .get_or_init(|| {
            temp_dir().join(
                rand::thread_rng()
                    .sample_iter(&rand::distributions::Alphanumeric)
                    .take(7)
                    .map(char::from)
                    .collect::<String>(),
            )
        })
        .clone()
}

/// Returns the `default_random_directory` appended with `deals_storage`.
fn default_storage_directory() -> PathBuf {
    default_random_directory().join("deals_storage")
}

/// Returns the `default_random_directory` appended with `deals_database`.
fn default_database_directory() -> PathBuf {
    default_random_directory().join("deals_database")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfiguration {
    /// The server's listen address.
    #[serde(default = "default_upload_listen_address")]
    pub(crate) listen_address: SocketAddr,

    /// The target parachain node's address.
    #[serde(default = "default_node_address")]
    pub(crate) node_url: Url,

    /// RocksDB storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/deals_database`.
    #[serde(default = "default_database_directory")]
    pub(crate) database_directory: PathBuf,

    /// Piece storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/deals_storage`.
    #[serde(default = "default_storage_directory")]
    pub(crate) storage_directory: PathBuf,

    /// The number of prove commits to be run in parallel.
    /// MUST BE > 0 or the pipeline will not progress.
    ///
    /// Creating a replica is memory-heavy process.
    /// E.g. With 2KiB sector sizes and 16GiB of RAM, it goes OOM at 4 parallel.
    #[serde(default = "default_parallel_prove_commits")]
    pub(crate) parallel_prove_commits: NonZero<usize>,

    /// Proof of Replication proof type.
    #[serde(default = "RegisteredSealProof::_2KiB")]
    pub(crate) seal_proof: RegisteredSealProof,

    /// Proof of Spacetime proof type.
    #[serde(default = "RegisteredPoStProof::_2KiB")]
    pub(crate) post_proof: RegisteredPoStProof,

    /// Proving Parameters for PoRep proof, corresponding to given `seal_proof` sector size.
    /// They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.
    ///
    /// Testing/temporary parameters can be generated via `polka-storage-provider-client proofs porep-params` command.
    /// Note that when you generate keys, for local testnet,
    /// **they need to be set** via an extrinsic pallet-proofs::set_porep_verifyingkey.
    pub(crate) porep_parameters: PathBuf,

    /// Proving Parameters for PoSt proof, corresponding to given `post_proof` sector size.
    /// They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.
    ///
    /// Testing/temporary parameters can be generated via `polka-storage-provider-client proofs post-params` command.
    /// Note that when you generate keys, for local testnet,
    /// **they need to be set** via an extrinsic pallet-proofs::set_post_verifyingkey.
    pub(crate) post_parameters: PathBuf,

    #[serde(default)]
    pub(crate) sealing_configuration: SealingConfiguration,
}

impl ServerConfiguration {
    pub fn from_path<P>(path: P) -> Result<Self, ServerError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref().canonicalize()?;
        match path.extension() {
            Some(ext) if ext == "toml" => {
                let config = std::fs::read_to_string(path)?;
                // NOTE: without the type annotation a warning about 2024 edition is issued
                Ok(toml::from_str::<ServerConfiguration>(&config)?)
            }
            Some(ext) if ext == "json" => Ok(serde_json::from_reader(std::fs::File::open(path)?)?),
            Some(_) => Err(ServerError::InvalidConfig("unsupported file format")),
            None => Err(ServerError::InvalidConfig("could not detect file format")),
        }
        .and_then(|config| {
            if config.post_proof.sector_size() != config.seal_proof.sector_size() {
                Err(ServerError::SectorSizeMismatch)
            } else {
                Ok(config)
            }
        })
    }
}
