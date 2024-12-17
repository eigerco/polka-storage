use libp2p::rendezvous::{client::RegisterError, ErrorCode, NamespaceTooLong};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("Failed to initialize node due to invalid TCP config")]
    InvalidTCPConfig,
    #[error("Failed to initialize node due to invalid behaviour config")]
    InvalidBehaviourConfig,
    #[error("Transport error. Failed to listen on given address")]
    ListenError,
    #[error("Transport error. Failed to dial to given address")]
    DialError,
    #[error("{0}")]
    NamespaceTooLong(#[from] NamespaceTooLong),
    #[error("{0}")]
    RegisterError(#[from] RegisterError),
    #[error("Registration failed: {0:?}")]
    RegistrationFailed(ErrorCode),
}
