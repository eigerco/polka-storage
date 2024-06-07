use clap::Parser;

use crate::commands::{InfoCommand, InitCommand, RunCommand};

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,
}

/// Supported sub-commands.
#[derive(Debug, clap::Subcommand, Clone)]
pub enum SubCommand {
    /// Initialize the polka storage provider
    Init(InitCommand),
    /// Start a polka storage provider
    Run(RunCommand),
    /// Info command
    Info(InfoCommand),
}
