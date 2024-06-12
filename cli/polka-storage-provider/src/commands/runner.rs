use clap::Parser;
use cli_primitives::Result;

use super::WalletCommand;
use crate::{cli::SubCommand, Cli};

/// Parses command line arguments into the service configuration and runs the specified
/// command with it.
pub(crate) async fn run() -> Result<()> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    match &cli_arguments.subcommand {
        SubCommand::Init(cmd) => cmd.run().await,
        SubCommand::Run(cmd) => cmd.run().await,
        SubCommand::Info(cmd) => cmd.run().await,
        SubCommand::Wallet(cmd) => match cmd {
            WalletCommand::GenerateNodeKey(cmd) => Ok(cmd.run()?),
            WalletCommand::Generate(cmd) => Ok(cmd.run()?),
            WalletCommand::Inspect(cmd) => Ok(cmd.run()?),
            WalletCommand::InspectNodeKey(cmd) => Ok(cmd.run()?),
            WalletCommand::Vanity(cmd) => Ok(cmd.run()?),
            WalletCommand::Verify(cmd) => Ok(cmd.run()?),
            WalletCommand::Sign(cmd) => Ok(cmd.run()?),
        },
    }
}
