use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZero,
    path::PathBuf,
};

use polka_storage_provider_common::config::sealing::SealingConfiguration;
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use serde::Deserialize;
use url::Url;

use crate::DEFAULT_NODE_ADDRESS;

/// Default address to bind the RPC server to.
const fn default_upload_listen_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001)
}

/// Default number of parallel prove commits.
const fn default_parallel_prove_commits() -> NonZero<usize> {
    // SAFETY: 2 != 0
    unsafe { NonZero::new_unchecked(2) }
}

fn default_node_address() -> Url {
    Url::parse(DEFAULT_NODE_ADDRESS).expect("DEFAULT_NODE_ADDRESS must be a valid Url")
}

#[derive(Debug, Clone, Deserialize)]
// #[serde(deny_unknown_fields)]
pub struct ConfigurationArgs {
    /// The server's listen address.
    #[serde(default = "default_upload_listen_address")]
    pub(crate) upload_listen_address: SocketAddr,

    /// The target parachain node's address.
    #[serde(default = "default_node_address")]
    pub(crate) node_url: Url,

    /// RocksDB storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/deals_database`.
    pub(crate) database_directory: Option<PathBuf>,

    /// Piece storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/...`.
    pub(crate) storage_directory: Option<PathBuf>,

    /// The number of prove commits to be run in parallel.
    /// MUST BE > 0 or the pipeline will not progress.
    ///
    /// Creating a replica is memory-heavy process.
    /// E.g. With 2KiB sector sizes and 16GiB of RAM, it goes OOM at 4 parallel.
    #[serde(default = "default_parallel_prove_commits")]
    pub(crate) parallel_prove_commits: NonZero<usize>,

    // NOTE: the following parameters are marked as "not required" so the CLI doesn't require them
    // when --config is used, otherwise, they're very much required
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

    pub(crate) public_secure_upload_url: Option<String>,

    #[serde(default)]
    pub(crate) sealing_configuration: SealingConfiguration,
}
