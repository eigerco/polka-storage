#![warn(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]

mod cmd;

use std::error::Error;

use clap::{ArgGroup, Parser, Subcommand};
use cmd::market::MarketCommand;
use storagext::runtime::balances::storage::types::account;
use subxt::{OnlineClient, SubstrateConfig};
use subxt_signer::{ecdsa, sr25519};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};
use url::Url;

pub(crate) const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO: replace the box/dyn
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

    match (cli_arguments.sr25519_key, cli_arguments.ecdsa_key) {
        (Some(account_keypair), _) => {
            run_cmd_with_keypair(
                cli_arguments.node_rpc,
                account_keypair,
                cli_arguments.subcommand,
            )
            .await?
        }
        (_, Some(account_keypair)) => {
            run_cmd_with_keypair(
                cli_arguments.node_rpc,
                account_keypair,
                cli_arguments.subcommand,
            )
            .await?
        }
        _ => unreachable!("should be handled by clap::ArgGroup"),
    }

    Ok(())
}

async fn run_cmd_with_keypair<Keypair>(
    node_rpc: Url,
    account_keypair: Keypair,
    cmd: SubCommand,
) -> Result<(), Box<dyn Error>>
where
    Keypair: subxt::tx::Signer<SubstrateConfig>,
{
    match cmd {
        SubCommand::Market(cmd) => {
            cmd.run(node_rpc, account_keypair).await?;
        }
    }

    Ok(())
}

fn parse_sr25519_keypair(mut src: &str) -> Result<sr25519::Keypair, String> {
    if src.starts_with("0x") {
        src = &src[2..]
    }
    let mut key_bytes = [0u8; 32];
    hex::decode_to_slice(src, &mut key_bytes).unwrap();
    Ok(sr25519::Keypair::from_secret_key(key_bytes).unwrap())
}

fn parse_ecdsa_keypair(mut src: &str) -> Result<ecdsa::Keypair, String> {
    if src.starts_with("0x") {
        src = &src[2..]
    }
    let mut key_bytes = [0u8; 32];
    hex::decode_to_slice(src, &mut key_bytes).unwrap();
    Ok(ecdsa::Keypair::from_secret_key(key_bytes).unwrap())
}

#[derive(Debug, Parser)]
#[command(group(ArgGroup::new("key").required(true).args(&["sr25519_key", "ecdsa_key"])))]
struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,

    /// URL of the providers RPC server.
    #[arg(long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub node_rpc: Url,

    /// An hex encoded Sr25519 key
    #[arg(long, value_parser = parse_sr25519_keypair)]
    pub sr25519_key: Option<sr25519::Keypair>,

    #[arg(long, value_parser = parse_ecdsa_keypair)]
    pub ecdsa_key: Option<ecdsa::Keypair>,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    // Perform market operations.
    #[command(subcommand)]
    Market(MarketCommand),
}
