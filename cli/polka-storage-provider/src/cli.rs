use clap::Parser;

use crate::commands::{InfoCommand, InitCommand, RunCommand, WalletCommand};

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,
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
