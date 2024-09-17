use std::io;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{0}")]
    MaterError(#[from] mater::Error),
    #[error("{0}")]
    IoError(#[from] io::Error),
    #[error("Supplied file does not have the appropriate metadata")]
    InvalidCarFile,
}
