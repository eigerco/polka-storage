use clap::Parser;

use super::WalletCommand;
use crate::{
    cli::{CliError, SubCommand},
    Cli,
};

/// Parses command line arguments into the service configuration and runs the
/// specified command with it.
pub(crate) async fn run() -> Result<(), CliError> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    // RPC client used to interact with the full node
    // let rpc_client = Client::new(cli_arguments.rpc_server_url).await?;

    // TODO(#274,@cernicc,26/08/2024): Uncomment commands when needed
    match &cli_arguments.subcommand {
        // SubCommand::Init(cmd) => cmd.run().await,
        // SubCommand::Run(cmd) => cmd.run().await,
        SubCommand::Storage(cmd) => cmd.run().await,
        // SubCommand::Info(cmd) => cmd.run(&rpc_client).await,
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
