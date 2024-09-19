mod info;
mod storage;
mod wallet;

use clap::Parser;
pub(crate) use info::InfoCommand;
pub(crate) use storage::StorageCommand;
pub(crate) use wallet::WalletCommand;

use crate::{rpc::Client, Cli, CliError, SubCommand};

/// Parses command line arguments into the service configuration and runs the
/// specified command with it.
pub(crate) async fn run() -> Result<(), CliError> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    // RPC client used to interact with the full node
    let rpc_client = Client::new(cli_arguments.rpc_server_url).await?;

    match &cli_arguments.subcommand {
        SubCommand::Storage(cmd) => cmd.run().await,
        SubCommand::Info(cmd) => cmd.run(&rpc_client).await,
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
