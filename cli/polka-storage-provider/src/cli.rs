use clap::Parser;

use crate::{
    commands::{InfoCommand, InitCommand, RunCommand, WalletCommand},
    rpc::RPC_SERVER_DEFAULT_BIND_ADDR,
};

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,
    #[arg(short, long, default_value = RPC_SERVER_DEFAULT_BIND_ADDR)]
    pub rpc_server_address: String,
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
