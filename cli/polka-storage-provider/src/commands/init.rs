use clap::Parser;
use cli_primitives::Result;
use tracing::info;

/// Command to initialize the storage provider.s
#[derive(Debug, Clone, Parser)]
pub(crate) struct InitCommand {}

impl InitCommand {
    pub async fn handle(&self) -> Result<()> {
        info!("Initializing polka storage provider...");
        // TODO(@cernicc,31/05/2024): Init needed configurations.
        // TODO(@cernicc,31/05/2024): Check if full node is synced
        info!("Provider initialized successfully. Start it with `polka-storage-provider run`");

        unimplemented!()
    }
}
