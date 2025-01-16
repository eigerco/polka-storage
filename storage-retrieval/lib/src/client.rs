use std::{collections::HashMap, path::Path, sync::Arc};

use beetswap::{Event, QueryId};
use cid::Cid;
use futures::StreamExt;
use ipld_core::codec::Codec;
use ipld_dagpb::{DagPbCodec, PbNode};
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_core::ConnectedPoint;
use libp2p_swarm::{ConnectionId, DialError, SwarmEvent};
use mater::{FileBlockstore, DAG_PB_CODE, RAW_CODE};
use thiserror::Error;
use tracing::{debug, error, info, instrument, trace};

use crate::{new_swarm, Behaviour, BehaviourEvent, InitSwarmError};

/// Errors that can occur while retrieving some content.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Error occurred while initialing swarm
    #[error("Swarm initialization error: {0}")]
    InitSwarm(#[from] InitSwarmError),
    /// This error indicates that the download was timed out.
    #[error("Download timeout")]
    DownloadTimeout,
    /// Error occurred when trying to establish or upgrade an outbound connection.
    #[error("Dial error: {0}")]
    Dial(#[from] DialError),
    /// Error produced by the mater
    #[error("Mater error: {0}")]
    Mater(#[from] mater::Error),
}

/// A client is used to download blocks from the storage provider. Single client
/// supports getting a single payload.
pub struct Client {
    /// Providers of data
    providers: Vec<Multiaddr>,
    /// Swarm instance
    swarm: Swarm<Behaviour<PassthroughBlockstore>>,
    /// The in flight block queries. If empty we know that the client received
    /// all requested data.
    queries: HashMap<QueryId, Cid>,
    /// Blockstore used by the client to store blocks into.
    blockstore: FileBlockstore,
    /// Content roots being downloaded.
    roots: Vec<Cid>,
}

impl Client {
    pub async fn new<P>(
        path: P,
        providers: Vec<Multiaddr>,
        roots: Vec<Cid>,
    ) -> Result<Self, ClientError>
    where
        P: AsRef<Path>,
    {
        // The p2p node which is created by the client doesn't need a real
        // blockstore. The reason is that the blockstore is only used by the
        // node when sharing blocks with other peers.
        let swarm = new_swarm(Arc::new(PassthroughBlockstore))?;

        // Blockstore used to store blocks in. The reason why we separated the
        // actual blockstore used by the client and the blockstore passed to the
        // swarm is, because the bitswap behaviour is adding blocks to the
        // blockstore in asynchronous manner. Because of that we couldn't know
        // when was the download actually finished.
        let blockstore = FileBlockstore::new(path, roots.clone()).await?;

        Ok(Self {
            providers,
            swarm,
            queries: HashMap::new(),
            blockstore,
            roots,
        })
    }

    /// Start download of some content with a payload cid.
    pub async fn download(mut self) -> Result<(), ClientError> {
        // Dial all providers
        for provider in self.providers.clone() {
            self.swarm.dial(provider)?;
        }

        // Start the download by requesting the roots of the trees.
        self.roots
            .clone()
            .into_iter()
            .for_each(|root| self.request_block(root));

        while let Some(event) = self.swarm.next().await {
            // Handle event received from the providers
            self.on_swarm_event(event).await?;

            // if no inflight queries, that means we received
            // everything requested. Finalize the blockstore.
            if self.queries.is_empty() {
                self.blockstore.finalize(None).await?;
                break;
            }
        }

        Ok(())
    }

    fn request_block(&mut self, cid: Cid) {
        debug!("requesting block {cid}");
        let query_id = self.swarm.behaviour_mut().bitswap.get(&cid);
        self.queries.insert(query_id, cid);
    }

    async fn on_swarm_event(
        &mut self,
        event: SwarmEvent<BehaviourEvent<PassthroughBlockstore>>,
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
                self.on_peer_disconnected(peer_id, connection_id);
            }
            SwarmEvent::Behaviour(BehaviourEvent::Bitswap(event)) => {
                self.on_bitswap_event(event).await?;
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

    #[instrument(level = "trace", skip(self))]
    async fn on_bitswap_event(&mut self, event: Event) -> Result<(), ClientError> {
        match event {
            Event::GetQueryResponse { query_id, data } => {
                let Some(cid) = self.queries.remove(&query_id) else {
                    return Ok(());
                };

                // Store the received block to a blockstore
                self.blockstore.put_keyed(&cid, &data).await?;
                info!("received new block {cid:?}");

                match cid.codec() {
                    DAG_PB_CODE => {
                        let node = <DagPbCodec as Codec<PbNode>>::decode_from_slice(&data).unwrap();

                        // Request a block for each new discoverd link
                        node.links
                            .iter()
                            .for_each(|link| self.request_block(link.cid));
                    }
                    RAW_CODE => {
                        debug!("{cid} raw block. nothing to do");
                    }
                    _other => {
                        error!("{cid} codec {_other} not supported");
                    }
                }
            }
            Event::GetQueryError { query_id, error } => {
                if let Some(cid) = self.queries.remove(&query_id) {
                    info!("received error for {cid:?}: {error}");
                }
            }
        }

        Ok(())
    }
}

/// The blockstore used by the client. It simulates a blockstore that never
/// holds any blocks.
struct PassthroughBlockstore;

impl blockstore::Blockstore for PassthroughBlockstore {
    async fn get<const S: usize>(
        &self,
        _cid: &cid::CidGeneric<S>,
    ) -> blockstore::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn put_keyed<const S: usize>(
        &self,
        _cid: &cid::CidGeneric<S>,
        _data: &[u8],
    ) -> blockstore::Result<()> {
        Ok(())
    }

    async fn remove<const S: usize>(&self, _cid: &cid::CidGeneric<S>) -> blockstore::Result<()> {
        Ok(())
    }

    async fn close(self) -> blockstore::Result<()> {
        Ok(())
    }
}
