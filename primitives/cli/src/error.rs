use thiserror::Error;

/// CLI components error handling implementor.
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Substrate error: {0}")]
    Substrate(#[from] subxt::Error),
}
