use crate::cli::Subcommand;
use crate::Cli;
use clap::Parser;
use cli_primitives::Result;

/// Parses command line arguments into the service configuration and runs the specified
/// command with it.
pub(crate) async fn run() -> Result<()> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    match &cli_arguments.subcommand {
        Some(Subcommand::RunRpc(_cmd)) => {
            // TODO(@serhii, #52, 2024-05-28): Implement an RPC server to listen for RPC calls, which will be used by the UI app to display information to the user.
            Ok(())
        }
        Some(Subcommand::StopRpc(_cmd)) => {
            // TODO(@serhii, #52, 2024-05-28): Implement functionality to gracefully stop the previously started RPC server.
            Ok(())
        }
        None => {
            // TODO(@serhii, #54, 2024-05-28): Add default logic for when no specific command is requested.
            Ok(())
        }
    }
}
