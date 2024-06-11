use thiserror::Error;

/// CLI components error handling implementor.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),
}
