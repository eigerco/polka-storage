use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZero,
    path::PathBuf,
    str::FromStr,
};

use clap::Args;
use libp2p::{identity::Keypair, Multiaddr, PeerId};
use polka_storage_provider_common::config::sealing::SealingConfiguration;
use primitives::{
    p2p::{keypair_value_parser, validate_tcp_multiaddr, validate_ws_multiaddr},
    proofs::{RegisteredPoStProof, RegisteredSealProof},
};
use serde::{de::Error, Deserialize, Deserializer};
use url::Url;

use crate::DEFAULT_NODE_ADDRESS;

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

fn default_p2p_tcp_multiaddr() -> Multiaddr {
    Multiaddr::from_str("/ip4/127.0.0.1/tcp/8002").expect("value should be a valid Multiaddr")
}

fn default_p2p_ws_multiaddr() -> Multiaddr {
    Multiaddr::from_str("/ip4/127.0.0.1/tcp/8003/ws").expect("value should be a valid Multiaddr")
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

    /// P2P ED25519 private key
    #[serde(deserialize_with = "deser_keypair")]
    #[arg(long, value_parser = keypair_value_parser, required = false)]
    pub(crate) p2p_key: Keypair,

    /// P2P TCP listen address.
    #[serde(default = "default_p2p_tcp_multiaddr")]
    #[arg(long, default_value_t = default_p2p_tcp_multiaddr(), value_parser = validate_tcp_multiaddr)]
    pub(crate) p2p_tcp_listen_address: Multiaddr,

    /// P2P websocket listen address.
    #[serde(default = "default_p2p_ws_multiaddr")]
    #[arg(long, default_value_t = default_p2p_ws_multiaddr(), value_parser = validate_ws_multiaddr)]
    pub(crate) p2p_ws_listen_address: Multiaddr,

    /// Rendezvous multiaddr that the node registers to.
    #[arg(long, required = false)]
    pub(crate) rendezvous_point_address: Multiaddr,

    /// PeerID of the rendezvous node used.
    #[serde(deserialize_with = "deserialize_string_to_peer_id")]
    #[arg(long, required = false)]
    pub(crate) rendezvous_point: PeerId,

    #[clap(flatten)]
    #[serde(default)]
    pub(crate) sealing_configuration: SealingConfiguration,
}

/// Deserializes a ED25519 private key into a Keypair.
/// Can either be the private key as a string or the path of a PEM file with an @ prefixed
/// Calls `keypair_value_parser` after deserializing the source string
fn deser_keypair<'de, D: Deserializer<'de>>(d: D) -> Result<Keypair, D::Error> {
    let src: String = Deserialize::deserialize(d)?;
    keypair_value_parser(&src).map_err(Error::custom)
}

/// Parses a string to an Peer ID.
fn deserialize_string_to_peer_id<'de, D: Deserializer<'de>>(d: D) -> Result<PeerId, D::Error> {
    let s: String = Deserialize::deserialize(d)?;
    PeerId::from_str(&s).map_err(Error::custom)
}
