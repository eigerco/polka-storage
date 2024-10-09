use jsonrpsee::core::ClientError;
use polka_storage_provider_common::rpc::StorageProviderRpcClient;
use storagext::{
    deser::DeserializablePath,
    multipair::{MultiPairArgs, MultiPairSigner},
    types::market::{ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal},
};
use url::Url;

use crate::rpc_client::PolkaStorageRpcClient;

/// Default RPC server's URL.
const DEFAULT_RPC_SERVER_URL: &str = "http://127.0.0.1:8000";

#[derive(Debug, thiserror::Error)]
pub enum ClientCommandError {
    #[error("the RPC client failed: {0}")]
    RpcClient(#[from] ClientError),

    #[error("no signer key was provider")]
    NoSigner,
}

#[derive(Debug, clap::Parser)]
pub struct ClientCommand {
    /// URL of the providers RPC server.
    #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
    pub rpc_server_url: Url,

    #[clap(subcommand)]
    pub command: ClientSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum ClientSubcommand {
    /// Retrieve information about the provider's node.
    Info,
    /// Propose a storage deal.
    ProposeDeal {
        /// Storage deal to propose. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <SxtDealProposal as DeserializablePath>::deserialize_json )]
        deal_proposal: SxtDealProposal,
    },
    /// Publish a signed storage deal.
    PublishDeal {
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

impl ClientCommand {
    pub async fn run(self) -> Result<(), ClientCommandError> {
        let client = PolkaStorageRpcClient::new(&self.rpc_server_url).await?;
        match self.command {
            ClientSubcommand::Info => {
                let info = client.info().await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&info)
                        .expect("type is serializable so this call should never fail")
                );
            }
            ClientSubcommand::ProposeDeal { deal_proposal } => {
                let result = client.propose_deal(deal_proposal).await?;
                println!("{}", result);
            }
            ClientSubcommand::PublishDeal {
                client_deal_proposal,
            } => {
                let result = client.publish_deal(client_deal_proposal).await?;
                println!("{}", result);
            }
            ClientSubcommand::SignDeal {
                deal_proposal,
                signer_key,
            } => {
                let Some(signer) = Option::<MultiPairSigner>::from(signer_key) else {
                    return Err(ClientCommandError::NoSigner);
                };

                let signature = deal_proposal.sign_serializable(&signer);

                println!(
                    "{}",
                    serde_json::to_string_pretty(&signature)
                        .expect("the type is serializable, so this should never fail")
                );
            }
        };
        Ok(())
    }
}
