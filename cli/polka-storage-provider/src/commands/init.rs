use clap::Parser;
use tracing::info;

use crate::Error;

/// Command to initialize the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct InitCommand;

impl InitCommand {
    pub async fn run(&self) -> Result<(), Error> {
        info!("Initializing polka storage provider...");
        // TODO(#64,@cernicc,31/05/2024): Init needed configurations.
        // TODO(#65,@cernicc,31/05/2024): Check if full node is synced
        info!("Provider initialized successfully. Start it with `polka-storage-provider run`");

        unimplemented!()
    }
}
