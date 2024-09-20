use std::fmt::{self, Display, Formatter};

use chrono::{DateTime, Utc};
use url::Url;

use crate::{
    rpc::{client::Client, requests::info::InfoRequest, version::V0},
    CliError,
};

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
}

impl ClientCommand {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let client = Client::new(self.rpc_server_url).await?;
        match self.command {
            ClientSubcommand::Info(cmd) => Ok(cmd.run(&client).await?),
        }
    }
}

/// Command to display information about the storage provider.
#[derive(Debug, Clone, clap::Parser)]
pub struct InfoCommand;

impl InfoCommand {
    pub async fn run(self, client: &Client<V0>) -> Result<(), CliError> {
        // TODO(#67,@cernicc,07/06/2024): Print polkadot address used by the provider

        // Get server info
        let server_info = client.execute(InfoRequest).await?;

        let node_status_info = NodeStatusInfo {
            start_time: server_info.start_time,
        };

        println!("{}", node_status_info);

        Ok(())
    }
}

struct NodeStatusInfo {
    start_time: DateTime<Utc>,
}

impl Display for NodeStatusInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Started at: {}", self.start_time)
    }
}
