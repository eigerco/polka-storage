use clap::Parser;
use cli_primitives::Result;

use crate::{cli::SubCommand, Cli};

/// Parses command line arguments into the service configuration and runs the specified
/// command with it.
pub(crate) async fn run() -> Result<()> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    match &cli_arguments.subcommand {
        SubCommand::Init(cmd) => cmd.handle().await,
        SubCommand::Run(cmd) => cmd.handle().await,
        SubCommand::Info(cmd) => cmd.handle().await,
    }
}
