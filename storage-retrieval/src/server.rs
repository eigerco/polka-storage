use std::{io, sync::Arc};

use blockstore::Blockstore;
use futures::StreamExt;
use libp2p::{Multiaddr, Swarm, TransportError};
use thiserror::Error;
use tracing::trace;

use crate::{new_swarm, Behaviour, InitSwarmError};

/// Error that can occur while running storage retrieval server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// Error occurred while initialing swarm
    #[error("Swarm initialization error: {0}")]
    InitSwarm(#[from] InitSwarmError),
    /// An error propagated from the libp2p transport.
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError<io::Error>),
}

/// Storage retrieval server. Server listens on the block requests and provide
/// them to the client.
pub struct Server<B>
where
    B: Blockstore + 'static,
{
    // Swarm instance
    swarm: Swarm<Behaviour<B>>,
}

impl<B> Server<B>
where
    B: Blockstore + 'static,
{
    pub fn new(blockstore: Arc<B>) -> Result<Self, ServerError> {
        let swarm = new_swarm(blockstore)?;

        Ok(Self { swarm })
    }

    // Start the server. The server can only stop if it received a cancellation
    // event or some error occurred.
    pub async fn run(mut self, listeners: Vec<Multiaddr>) -> Result<(), ServerError> {
        // Listen on
        for listener in listeners {
            self.swarm.listen_on(listener)?;
        }

        // Keep server running
        loop {
            let event = self.swarm.select_next_some().await;
            trace!(?event, "Received swarm event");
        }
    }
}
