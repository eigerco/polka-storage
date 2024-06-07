use clap::Parser;
use cli_primitives::Result;

/// Command to display information about the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct InfoCommand {}

impl InfoCommand {
    pub async fn handle(&self) -> Result<()> {
        // TODO(@cernicc,31/05/2024): Print providers information
        unimplemented!()
    }
}
