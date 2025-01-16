use std::{io, sync::Arc};

use blockstore::Blockstore;
use futures::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm, TransportError};
use libp2p_core::ConnectedPoint;
use libp2p_swarm::{ConnectionId, SwarmEvent};
use thiserror::Error;
use tracing::{debug, instrument, trace};

use crate::{new_swarm, Behaviour, BehaviourEvent, InitSwarmError};

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

    // Start the server. The server will stop if it received a cancellation
    // event or some error occurred.
    pub async fn run(mut self, listeners: Vec<Multiaddr>) -> Result<(), ServerError> {
        // Listen on
        for listener in listeners {
            self.swarm.listen_on(listener)?;
        }

        // Keep server running
        loop {
            let event = self.swarm.select_next_some().await;
            self.on_swarm_event(event)?;
        }
    }

    fn on_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent<B>>) -> Result<(), ServerError> {
        trace!(?event, "Received swarm event");

        match event {
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            } => {
                self.on_peer_connected(peer_id, connection_id, endpoint);
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                ..
            } => {
                self.on_peer_disconnected(peer_id, connection_id);
            }
            _ => {
                // Nothing to do here
            }
        }

        Ok(())
    }

    #[instrument(skip_all, fields(peer_id = %peer_id))]
    fn on_peer_connected(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        _endpoint: ConnectedPoint,
    ) {
        debug!("Peer connected");
    }

    #[instrument(skip_all, fields(peer_id = %peer_id))]
    fn on_peer_disconnected(&mut self, peer_id: PeerId, _connection_id: ConnectionId) {
        debug!("Peer disconnected");
    }
}
