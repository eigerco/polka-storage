use crate::commands::{RunRpcCmd, StopRpcCmd};
use clap::Parser;

/// A CLI application that facilitates management operations over a running full node and other components.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,
}

/// Supported sub-commands.
#[derive(Debug, clap::Subcommand, Clone)]
pub enum Subcommand {
    /// Command to run the RPC server.
    RunRpc(RunRpcCmd),
    /// Command to stop the RPC server.
    StopRpc(StopRpcCmd),
}
