//! A CLI application that facilitates management operations over a running full node and other components.
#![deny(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]
// TODO(#274,@cernicc,26/08/2024): Remove after #274 is done
#![allow(dead_code)]

pub(crate) mod commands;
mod rpc;
mod storage;

use clap::Parser;
use thiserror::Error;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use crate::{
    commands::{InfoCommand, StorageCommand, WalletCommand},
    rpc::{server::RPC_SERVER_DEFAULT_BIND_ADDR, ClientError},
};

#[tokio::main]
async fn main() -> Result<(), CliError> {
    // Logger initialization.
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .init();

    // Run requested command.
    commands::run().await
}

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,

    /// URL of the providers RPC server.
    #[arg(long, default_value_t = Url::parse(&format!("http://{RPC_SERVER_DEFAULT_BIND_ADDR}")).unwrap())]
    pub rpc_server_url: Url,
}

/// Supported sub-commands.
#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
    Storage(StorageCommand),

    /// Info command to display information about the storage provider.
    Info(InfoCommand),

    /// Command to manage wallet operations.
    #[command(subcommand)]
    Wallet(WalletCommand),
}

/// CLI components error handling implementor.
#[derive(Debug, Error)]
pub enum CliError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FromEnv error: {0}")]
    EnvError(#[from] tracing_subscriber::filter::FromEnvError),

    #[error("URL parse error: {0}")]
    ParseUrl(#[from] url::ParseError),

    #[error("Substrate error: {0}")]
    Substrate(#[from] subxt::Error),

    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),

    #[error("Rpc Client error: {0}")]
    RpcClient(#[from] ClientError),
}
