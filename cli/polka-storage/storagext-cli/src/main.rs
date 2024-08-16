#![deny(clippy::unwrap_used)]

mod cmd;
mod deser;

use std::fmt::Debug;

use clap::{ArgGroup, Parser, Subcommand};
use cmd::{market::MarketCommand, storage_provider::StorageProviderCommand};
use deser::{DealProposal, DebugPair};
use storagext::PolkaStorageConfig;
use subxt::ext::sp_core::{
    ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};
use url::Url;

pub(crate) const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:42069";

#[derive(Debug, Parser)]
#[command(group(ArgGroup::new("keypair").required(true).args(
    &["sr25519_key", "ecdsa_key", "ed25519_key"]
)))]
struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,

    /// RPC server's URL.
    #[arg(long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub node_rpc: Url,

    /// Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, env, value_parser = DebugPair::<Sr25519Pair>::value_parser)]
    pub sr25519_key: Option<DebugPair<Sr25519Pair>>,

    /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, env, value_parser = DebugPair::<ECDSAPair>::value_parser)]
    pub ecdsa_key: Option<DebugPair<ECDSAPair>>,

    /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, env, value_parser = DebugPair::<Ed25519Pair>::value_parser)]
    pub ed25519_key: Option<DebugPair<Ed25519Pair>>,
}

#[derive(Debug, Subcommand)]
enum SubCommand {
    // Perform market operations.
    #[command(subcommand)]
    Market(MarketCommand),
    #[command(subcommand)]
    StorageProvider(StorageProviderCommand),
}

impl SubCommand {
    async fn run_with_keypair<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), anyhow::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        match self {
            SubCommand::Market(cmd) => {
                cmd.run(node_rpc, account_keypair).await?;
            }
            SubCommand::StorageProvider(cmd) => {
                cmd.run(node_rpc, account_keypair).await?;
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Logger initialization.
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_filter(
                    EnvFilter::builder()
                        .with_default_directive(LevelFilter::INFO.into())
                        .from_env()?,
                )
                .with_filter(filter::filter_fn(|metadata| {
                    if let Some(module_path) = metadata.module_path() {
                        module_path.starts_with("storagext")
                    } else {
                        true
                    }
                })),
        )
        .init();

    let cli_arguments = Cli::parse();

    match (
        cli_arguments.sr25519_key,
        cli_arguments.ecdsa_key,
        cli_arguments.ed25519_key,
    ) {
        (Some(account_keypair), _, _) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        (_, Some(account_keypair), _) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        (_, _, Some(account_keypair)) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        _ => unreachable!("should be handled by clap::ArgGroup"),
    }

    Ok(())
}
