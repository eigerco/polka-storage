use std::io;

use libp2p::{noise, swarm::DialError, PeerId, TransportError};
use thiserror::Error;

/// Representation of all the errors that can occur when interacting with [`P2p`].
#[derive(Debug, Error)]
pub enum P2pError {
    /// Failed to initialize noise protocol.
    #[error("Failed to initialize noise: {0}")]
    InitNoise(String),

    /// Error occured when trying to establish or upgrade an outbound connection.
    #[error("Dial error: {0}")]
    Dial(#[from] DialError),

    /// An error propagated from the libp2p transport.
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError<io::Error>),

    /// Returned when the registration with some rendezvous node fails
    #[error("Failed to register with peer: {0} error: {1}")]
    RegistrationFailed(PeerId, String),

    /// An error that happens when no rendezvous nodes are available
    #[error("No rendezvous nodes available")]
    NoRendezvousNodesAvailable,
}

impl From<noise::Error> for P2pError {
    fn from(e: noise::Error) -> Self {
        P2pError::InitNoise(e.to_string())
    }
}
