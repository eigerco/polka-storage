use crate::Error;
use clap::Parser;

/// Command to display information about the storage provider.
#[derive(Debug, Clone, Parser)]
pub(crate) struct InfoCommand;

impl InfoCommand {
    pub async fn run(&self) -> Result<(), Error> {
        // TODO(#66,@cernicc,31/05/2024): Print start time of the provider
        // TODO(#67,@cernicc,07/06/2024): Print polkadot address used by the provider
        unimplemented!()
    }
}
