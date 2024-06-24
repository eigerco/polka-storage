use thiserror::Error;

/// CLI components error handling implementor.
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FromEnv error: {0}")]
    EnvError(#[from] tracing_subscriber::filter::FromEnvError),

    #[error("Substrate error: {0}")]
    Substrate(#[from] subxt::Error),

    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),
}
