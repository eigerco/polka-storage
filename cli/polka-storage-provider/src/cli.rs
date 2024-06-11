use clap::Parser;

use crate::commands::{RunRpcCmd, StopRpcCmd, WalletCmd};

/// A CLI application that facilitates management operations over a running full node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,
}

/// Supported sub-commands.
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
    /// Command to run the RPC server.
    RunRpc(RunRpcCmd),
    /// Command to stop the RPC server.
    StopRpc(StopRpcCmd),
    /// Command to manage wallet operations.
    #[command(subcommand)]
    Wallet(WalletCmd),
}
