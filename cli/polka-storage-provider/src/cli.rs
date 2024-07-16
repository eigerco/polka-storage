use clap::Parser;
use thiserror::Error;
use url::Url;

use crate::{
    commands::{DealProposalCommand, InfoCommand, InitCommand, RunCommand, StorageCommand, WalletCommand},
    rpc::{server::RPC_SERVER_DEFAULT_BIND_ADDR, ClientError},
};

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
    /// Initialize the polka storage provider
    Init(InitCommand),
    /// Start a polka storage provider
    Run(RunCommand),
    /// Command to start storage server.
    Storage(StorageCommand),
    /// Info command to display information about the storage provider.
    Info(InfoCommand),
    /// Command to generate a deal proposal by one of 3 guys  
    Deal(DealProposalCommand),
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
