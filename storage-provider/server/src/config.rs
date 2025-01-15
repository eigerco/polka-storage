use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZero,
    path::PathBuf,
};

use clap::Args;
use libp2p::{identity::Keypair, Multiaddr, PeerId};
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use serde::Deserialize;
use url::Url;

use crate::{
    p2p::{deser_keypair, keypair_value_parser, string_to_peer_id_option, NodeType},
    DEFAULT_NODE_ADDRESS,
};

/// Default address to bind the RPC server to.
const fn default_rpc_listen_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8000)
}

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

#[derive(Debug, Clone, Deserialize, Args)]
#[group(multiple = true, conflicts_with = "config")]
#[serde(deny_unknown_fields)]
pub struct ConfigurationArgs {
    /// The server's listen address.
    #[serde(default = "default_upload_listen_address")]
    #[arg(long, default_value_t = default_upload_listen_address())]
    pub(crate) upload_listen_address: SocketAddr,

    /// The server's listen address.
    #[serde(default = "default_rpc_listen_address")]
    #[arg(long, default_value_t = default_rpc_listen_address())]
    pub(crate) rpc_listen_address: SocketAddr,

    /// The target parachain node's address.
    #[serde(default = "default_node_address")]
    #[arg(long, default_value_t = default_node_address())]
    pub(crate) node_url: Url,

    /// RocksDB storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/deals_database`.
    #[arg(long)]
    pub(crate) database_directory: Option<PathBuf>,

    /// Piece storage directory.
    /// Defaults to a temporary random directory, like `/tmp/<random>/...`.
    #[arg(long)]
    pub(crate) storage_directory: Option<PathBuf>,

    /// The number of prove commits to be run in parallel.
    /// MUST BE > 0 or the pipeline will not progress.
    ///
    /// Creating a replica is memory-heavy process.
    /// E.g. With 2KiB sector sizes and 16GiB of RAM, it goes OOM at 4 parallel.
    #[serde(default = "default_parallel_prove_commits")]
    #[arg(long, default_value_t = default_parallel_prove_commits())]
    pub(crate) parallel_prove_commits: NonZero<usize>,

    // NOTE: the following parameters are marked as "not required" so the CLI doesn't require them
    // when --config is used, otherwise, they're very much required
    /// Proof of Replication proof type.
    #[serde(default = "RegisteredSealProof::_2KiB")]
    #[arg(long, required = false)]
    pub(crate) seal_proof: RegisteredSealProof,

    /// Proof of Spacetime proof type.
    #[serde(default = "RegisteredPoStProof::_2KiB")]
    #[arg(long, required = false)]
    pub(crate) post_proof: RegisteredPoStProof,

    /// Proving Parameters for PoRep proof, corresponding to given `seal_proof` sector size.
    /// They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.
    ///
    /// Testing/temporary parameters can be generated via `polka-storage-provider-client proofs porep-params` command.
    /// Note that when you generate keys, for local testnet,
    /// **they need to be set** via an extrinsic pallet-proofs::set_porep_verifyingkey.
    #[arg(long, required = false)]
    pub(crate) porep_parameters: PathBuf,

    /// Proving Parameters for PoSt proof, corresponding to given `post_proof` sector size.
    /// They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.
    ///
    /// Testing/temporary parameters can be generated via `polka-storage-provider-client proofs post-params` command.
    /// Note that when you generate keys, for local testnet,
    /// **they need to be set** via an extrinsic pallet-proofs::set_post_verifyingkey.
    #[arg(long, required = false)]
    pub(crate) post_parameters: PathBuf,

    /// P2P Node type, can be either a bootstrap node or a registration node.
    #[serde(default = "NodeType::default")]
    #[arg(long, default_value = "bootstrap")]
    pub(crate) node_type: NodeType,

    /// P2P ED25519 private key
    #[arg(long, value_parser = keypair_value_parser)]
    #[serde(deserialize_with = "deser_keypair")]
    pub(crate) p2p_key: Keypair,

    /// Rendezvous point address that the registration node connects to
    /// or the bootstrap node binds to.
    #[arg(long)]
    pub(crate) rendezvous_point_address: Multiaddr,

    /// PeerID of the bootstrap node used by the registration node.
    /// Optional because it is not used by the bootstrap node.
    #[arg(long)]
    #[serde(default, deserialize_with = "string_to_peer_id_option")]
    pub(crate) rendezvous_point: Option<PeerId>,
}
