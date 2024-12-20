use std::{collections::HashMap, sync::Arc};

use beetswap::QueryId;
use blockstore::Blockstore;
use cid::Cid;
use futures::{pin_mut, Future, StreamExt};
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_core::ConnectedPoint;
use libp2p_swarm::{ConnectionId, DialError, SwarmEvent};
use thiserror::Error;
use tracing::{debug, info, instrument, trace};

use crate::{new_swarm, Behaviour, BehaviourEvent, InitSwarmError};

#[derive(Debug, Error)]
pub enum ClientError {
    /// Error occurred while initialing swarm
    #[error("Swarm initialization error: {0}")]
    InitSwarm(#[from] InitSwarmError),
    /// Error occurred when trying to establish or upgrade an outbound connection.
    #[error("Dial error: {0}")]
    Dial(#[from] DialError),
    /// This error indicates that the download was canceled
    #[error("Download canceled")]
    DownloadCanceled,
}

/// A client is used to download blocks from the storage provider. Single client
/// supports getting a single payload.
pub struct Client<B>
where
    B: Blockstore + 'static,
{
    // Providers of data
    providers: Vec<Multiaddr>,
    // Swarm instance
    swarm: Swarm<Behaviour<B>>,
    /// The in flight block queries. If empty we know that the client received
    /// all requested data.
    queries: HashMap<QueryId, Cid>,
}

impl<B> Client<B>
where
    B: Blockstore,
{
    pub fn new(blockstore: Arc<B>, providers: Vec<Multiaddr>) -> Result<Self, ClientError> {
        let swarm = new_swarm(blockstore)?;

        Ok(Self {
            providers,
            swarm,
            queries: HashMap::new(),
        })
    }

    /// Start download of some content with a payload cid.
    pub async fn download(
        mut self,
        payload_cid: Cid,
        cancellation: impl Future<Output = ()>,
    ) -> Result<(), ClientError> {
        // Dial all providers
        for provider in self.providers.clone() {
            self.swarm.dial(provider)?;
        }

        // Request the root node of the car file
        let query_id = self.swarm.behaviour_mut().bitswap.get(&payload_cid);
        self.queries.insert(query_id, payload_cid);

        // Pin cancellation future
        pin_mut!(cancellation);

        loop {
            tokio::select! {
                // Data download was canceled
                _ = &mut cancellation => {
                    // Return an error as indication that the download was cancelled
                    return Err(ClientError::DownloadCanceled);
                }
                // Handle events received when we get some blocks back
                event = self.swarm.select_next_some() => {
                    // Handle event received from the providers
                    self.on_swarm_event(event).await?;

                    // if no inflight queries, that means we received
                    // everything requested.
                    if self.queries.is_empty() {
                        info!("Download of payload {payload_cid} finished");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn on_swarm_event(
        &mut self,
        event: SwarmEvent<BehaviourEvent<B>>,
    ) -> Result<(), ClientError> {
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
                self.on_peer_disconnected(peer_id, connection_id)?;
            }
            SwarmEvent::Behaviour(BehaviourEvent::Bitswap(event)) => {
                self.on_bitswap_event(event)?;
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

        // TODO: Track connections to the storage providers. We need statuses so
        // that we know if there is still some peer viable to download data
        // from.
    }

    #[instrument(skip_all, fields(peer_id = %peer_id))]
    fn on_peer_disconnected(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
    ) -> Result<(), ClientError> {
        debug!("Peer disconnected");

        // TODO: Remove connection from tracked. If there are no established
        // connections return an error. The download can never finish.

        Ok(())
    }

    fn on_bitswap_event(&mut self, event: beetswap::Event) -> Result<(), ClientError> {
        match event {
            beetswap::Event::GetQueryResponse { query_id, data } => {
                if let Some(cid) = self.queries.remove(&query_id) {
                    info!("received response for {cid:?}: {data:?}");
                }

                // TODO: Extract linked blocks from the cid. Then request those
                // new unknown blocks from the providers. Received blocks are
                // added automatically to the blockstore used by the client.

                // TODO: Figure out how the sequence of blocks is guaranteed. Do
                // we request each of them in sequence and wait for each of them
                // before requesting for a new one? Is there a better way?
            }
            beetswap::Event::GetQueryError { query_id, error } => {
                if let Some(cid) = self.queries.remove(&query_id) {
                    info!("received error for {cid:?}: {error}");
                }

                // TODO: Track errors for blocks. There is a case when no
                // providers can have a requested block. In that case we
                // should return an error and cancel download.
            }
        }

        Ok(())
    }
}
