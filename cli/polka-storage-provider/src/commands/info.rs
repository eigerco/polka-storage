use std::fmt::{self, Display, Formatter};

use chrono::{DateTime, Utc};
use clap::Parser;

use crate::{
    cli::CliError,
    rpc::{methods::common::InfoRequest, version::V0, Client},
};

/// Command to display information about the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct InfoCommand;

impl InfoCommand {
    pub async fn run(&self, client: &Client<V0>) -> Result<(), CliError> {
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
