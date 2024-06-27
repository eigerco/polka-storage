use std::str::FromStr;

use clap::Parser;
use url::Url;

use super::WalletCommand;
use crate::{cli::SubCommand, rpc::Client, Cli, Error};

/// Parses command line arguments into the service configuration and runs the
/// specified command with it.
pub(crate) async fn run() -> Result<(), Error> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    // RPC client used to interact with the full node
    let rpc_url = Url::from_str(&cli_arguments.rpc_server_url)?;
    let rpc_client = Client::new(rpc_url).await?;

    match &cli_arguments.subcommand {
        SubCommand::Init(cmd) => cmd.run().await,
        SubCommand::Run(cmd) => cmd.run().await,
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
