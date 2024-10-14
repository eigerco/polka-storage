mod client;
mod utils;
mod wallet;

use clap::Parser;

use self::{client::ClientCommand, utils::UtilsCommand};
pub(super) use crate::commands::wallet::WalletCommand;

/// CLI components error handling implementor.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FromEnv error: {0}")]
    EnvError(#[from] tracing_subscriber::filter::FromEnvError),

    #[error("URL parse error: {0}")]
    ParseUrl(#[from] url::ParseError),

    #[error("Substrate error: {0}")]
    Substrate(#[from] subxt::Error),

    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),

    #[error("Error occurred while working with a car file: {0}")]
    MaterError(#[from] mater::Error),

    #[error(transparent)]
    RpcCommand(#[from] crate::commands::client::ClientCommandError),

    #[error(transparent)]
    UtilsCommand(#[from] crate::commands::utils::UtilsCommandError),
}

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) enum Cli {
    /// Command to manage wallet operations.
    #[command(subcommand)]
    Wallet(WalletCommand),

    /// Client RPC commands.
    Client(ClientCommand),

    /// Utility commands for storage related actions.
    #[command(subcommand)]
    Utils(UtilsCommand),
}

impl Cli {
    /// Parses command line arguments into the service configuration and runs the
    /// specified command with it.
    pub(crate) async fn run() -> Result<(), CliError> {
        // CLI arguments parsed and mapped to the struct.
        let cli_arguments: Cli = Cli::parse();

        match cli_arguments {
            Self::Wallet(cmd) => match cmd {
                WalletCommand::GenerateNodeKey(cmd) => Ok(cmd.run()?),
                WalletCommand::Generate(cmd) => Ok(cmd.run()?),
                WalletCommand::Inspect(cmd) => Ok(cmd.run()?),
                WalletCommand::InspectNodeKey(cmd) => Ok(cmd.run()?),
                WalletCommand::Vanity(cmd) => Ok(cmd.run()?),
                WalletCommand::Verify(cmd) => Ok(cmd.run()?),
                WalletCommand::Sign(cmd) => Ok(cmd.run()?),
            },
            Self::Client(cmd) => Ok(cmd.run().await?),
            Self::Utils(utils) => Ok(utils.run().await?),
        }
    }
}
