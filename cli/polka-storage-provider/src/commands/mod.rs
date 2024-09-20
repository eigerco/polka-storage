mod rpc;
mod storage;
mod wallet;

use clap::Parser;

pub(super) use crate::commands::{rpc::RpcCommand, storage::StorageCommand, wallet::WalletCommand};
use crate::{Cli, SubCommand};

/// Parses command line arguments into the service configuration and runs the
/// specified command with it.
pub(crate) async fn run() -> Result<(), anyhow::Error> {
    // CLI arguments parsed and mapped to the struct.
    let cli_arguments: Cli = Cli::parse();

    match cli_arguments.subcommand {
        SubCommand::Storage(cmd) => Ok(cmd.run().await?),
        SubCommand::Wallet(cmd) => match cmd {
            WalletCommand::GenerateNodeKey(cmd) => Ok(cmd.run()?),
            WalletCommand::Generate(cmd) => Ok(cmd.run()?),
            WalletCommand::Inspect(cmd) => Ok(cmd.run()?),
            WalletCommand::InspectNodeKey(cmd) => Ok(cmd.run()?),
            WalletCommand::Vanity(cmd) => Ok(cmd.run()?),
            WalletCommand::Verify(cmd) => Ok(cmd.run()?),
            WalletCommand::Sign(cmd) => Ok(cmd.run()?),
        },
        SubCommand::Rpc(rpc) => Ok(rpc.run().await?),
    }
}
