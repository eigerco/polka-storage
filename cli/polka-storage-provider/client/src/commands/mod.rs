mod proofs;
mod wallet;

use clap::Parser;
use jsonrpsee::core::ClientError;
use polka_storage_provider_common::rpc::StorageProviderRpcClient;
use storagext::{
    deser::DeserializablePath,
    multipair::{MultiPairArgs, MultiPairSigner},
    types::market::{ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal},
};
use url::Url;

use self::proofs::ProofsCommand;
pub(super) use crate::commands::wallet::WalletCommand;
use crate::rpc_client::PolkaStorageRpcClient;

/// Default RPC server's URL.
const DEFAULT_RPC_SERVER_URL: &str = "http://127.0.0.1:8000";

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
    UtilsCommand(#[from] crate::commands::proofs::UtilsCommandError),

    #[error("the RPC client failed: {0}")]
    RpcClient(#[from] ClientError),

    #[error("no signer key was provider")]
    NoSigner,
}

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) enum Cli {
    /// Command to manage wallet operations.
    #[command(subcommand)]
    Wallet(WalletCommand),

    /// Utility commands for storage related actions.
    #[command(subcommand)]
    Proofs(ProofsCommand),

    /// Retrieve information about the provider's node.
    Info {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
    },

    /// Propose a storage deal.
    ProposeDeal {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
        /// Storage deal to propose. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <SxtDealProposal as DeserializablePath>::deserialize_json )]
        deal_proposal: SxtDealProposal,
    },

    /// Publish a signed storage deal.
    PublishDeal {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
        /// Storage deal to publish. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <SxtClientDealProposal as DeserializablePath>::deserialize_json)]
        client_deal_proposal: SxtClientDealProposal,
    },

    /// Sign a storage deal using the provided key, will output the deal as a JSON
    /// — no information is shared across the network.
    SignDeal {
        #[arg(value_parser = <SxtDealProposal as DeserializablePath>::deserialize_json)]
        deal_proposal: SxtDealProposal,

        #[command(flatten)]
        signer_key: MultiPairArgs,
    },
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
            Self::Proofs(utils) => Ok(utils.run().await?),
            Self::Info { rpc_server_url } => Self::info(rpc_server_url).await,
            Self::ProposeDeal {
                rpc_server_url,
                deal_proposal,
            } => Self::propose_deal(rpc_server_url, deal_proposal).await,
            Self::PublishDeal {
                rpc_server_url,
                client_deal_proposal,
            } => Self::publish_deal(rpc_server_url, client_deal_proposal).await,
            Self::SignDeal {
                deal_proposal,
                signer_key,
            } => Self::sign_deal(deal_proposal, signer_key),
        }
    }

    async fn info(rpc_server_url: Url) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let info = client.info().await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&info)
                .expect("type is serializable so this call should never fail")
        );
        Ok(())
    }

    async fn propose_deal(
        rpc_server_url: Url,
        deal_proposal: SxtDealProposal,
    ) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let result = client.propose_deal(deal_proposal).await?;
        println!("{}", result);
        Ok(())
    }

    async fn publish_deal(
        rpc_server_url: Url,
        client_deal_proposal: SxtClientDealProposal,
    ) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let result = client.publish_deal(client_deal_proposal).await?;
        println!("{}", result);
        Ok(())
    }

    fn sign_deal(
        deal_proposal: SxtDealProposal,
        signer_key: MultiPairArgs,
    ) -> Result<(), CliError> {
        let Some(signer) = Option::<MultiPairSigner>::from(signer_key) else {
            return Err(CliError::NoSigner);
        };

        let signature = deal_proposal.sign_serializable(&signer);

        println!(
            "{}",
            serde_json::to_string_pretty(&signature)
                .expect("the type is serializable, so this should never fail")
        );
        Ok(())
    }
}
