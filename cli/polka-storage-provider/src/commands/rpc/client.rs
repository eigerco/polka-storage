use storagext::{
    deser::DeserializablePath,
    multipair::{MultiPairArgs, MultiPairSigner},
    types::market::{ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal},
};
use url::Url;

use crate::rpc::{
    client::{Client, ClientError},
    requests::{deal_proposal::RegisterDealProposalRequest, info::InfoRequest},
    version::V0,
};

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
    // TODO(#398): replace the address with a constant
    #[arg(long, default_value = "http://127.0.0.1:8000")]
    pub rpc_server_url: Url,

    #[clap(subcommand)]
    pub command: ClientSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum ClientSubcommand {
    Info(InfoCommand),
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
        let client = Client::new(self.rpc_server_url).await?;
        match self.command {
            ClientSubcommand::Info(cmd) => Ok(cmd.run(&client).await?),
            ClientSubcommand::PublishDeal {
                client_deal_proposal,
            } => {
                let result = client
                    .execute(RegisterDealProposalRequest::from(client_deal_proposal))
                    .await?;
                println!("{}", result.to_string());
                Ok(())
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
                Ok(())
            }
        }
    }
}

/// Command to display information about the storage provider.
#[derive(Debug, Clone, clap::Parser)]
pub struct InfoCommand;

impl InfoCommand {
    pub async fn run(self, client: &Client<V0>) -> Result<(), ClientCommandError> {
        // TODO(#67,@cernicc,07/06/2024): Print polkadot address used by the provider

        // Get server info
        let server_info = client.execute(InfoRequest).await?;
        println!("Started at: {}", server_info.start_time);

        Ok(())
    }
}
