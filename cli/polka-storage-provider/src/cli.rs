use clap::Parser;
use url::Url;

use crate::{
    commands::{InfoCommand, InitCommand, RunCommand, WalletCommand},
    rpc::server::RPC_SERVER_DEFAULT_BIND_ADDR,
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
    /// Info command to display information about the storage provider.
    Info(InfoCommand),
    /// Command to manage wallet operations.
    #[command(subcommand)]
    Wallet(WalletCommand),
}
