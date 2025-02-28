use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use beetswap::{Event, QueryId};
use blockstore::Blockstore;
use cid::Cid;
use futures::StreamExt;
use ipld_core::codec::Codec;
use ipld_dagpb::{DagPbCodec, PbNode};
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_core::ConnectedPoint;
use libp2p_swarm::{ConnectionId, DialError, SwarmEvent};
use mater::{blockstore::ReadWriteBlockstore, FileReader, DAG_PB_CODE, RAW_CODE};
use thiserror::Error;
use tokio::{
    fs::{File, OpenOptions},
    io::AsyncSeekExt,
};
use tracing::{debug, error, info, instrument, trace};

use crate::p2p::{new_swarm, Behaviour, BehaviourEvent, InitSwarmError};

/// Errors that can occur while retrieving some content.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Error occurred while initialing swarm
    #[error("Swarm initialization error: {0}")]
    InitSwarm(#[from] InitSwarmError),
    /// Error occurred when trying to establish or upgrade an outbound connection.
    #[error("Dial error: {0}")]
    Dial(#[from] DialError),
    /// Error produced by the mater
    #[error("Mater error: {0}")]
    Mater(#[from] mater::Error),
    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Blockstore error.
    #[error(transparent)]
    Blockstore(#[from] blockstore::Error),
}

pub struct ClientSettings {
    output: PathBuf,
    extract: bool,
    overwrite: bool,
}

impl ClientSettings {
    pub fn new(output: PathBuf, extract: bool, overwrite: bool) -> Self {
        Self {
            output,
            extract,
            overwrite,
        }
    }
}

/// A client is used to download blocks from the storage provider. Single client
/// supports getting a single payload.
pub struct Client {
    settings: ClientSettings,

    /// Providers of data
    providers: Vec<Multiaddr>,
    /// Swarm instance
    swarm: Swarm<Behaviour<PassthroughBlockstore>>,
    /// The in flight block queries. If empty we know that the client received
    /// all requested data.
    queries: HashMap<QueryId, Cid>,
    /// Blockstore used by the client to store blocks into.
    blockstore: ReadWriteBlockstore<File>,
    /// Content roots being downloaded.
    root: Cid,
    /// CAR block DAG mapping children to parents. (The A in DAG isn't checked!)
    dag: HashMap<Cid, Cid>,
}

impl Client {
    pub async fn new(
        providers: Vec<Multiaddr>,
        root: Cid,
        settings: ClientSettings,
    ) -> Result<Self, ClientError> {
        // The p2p node which is created by the client doesn't need a real
        // blockstore. The reason is that the blockstore is only used by the
        // node when sharing blocks with other peers.
        let swarm = new_swarm(Arc::new(PassthroughBlockstore))?;

        // Blockstore used to store blocks in. The reason why we separated the
        // actual blockstore used by the client and the blockstore passed to the
        // swarm is, because the bitswap behaviour is adding blocks to the
        // blockstore in asynchronous manner. Because of that we couldn't know
        // when was the download actually finished.
        let mut car_path = settings.output.clone();
        car_path.set_extension("car");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .create_new(!settings.overwrite)
            .read(true)
            .open(car_path.clone())
            .await?;
        let blockstore = ReadWriteBlockstore::new(file).await?;

        Ok(Self {
            settings,
            providers,
            swarm,
            queries: HashMap::new(),
            blockstore,
            root,
            dag: HashMap::new(),
        })
    }

    /// Start download of some content with a payload cid.
    pub async fn download(mut self) -> Result<(), ClientError> {
        // Dial all providers
        for provider in self.providers.clone() {
            self.swarm.dial(provider)?;
        }

        // Start the download by requesting the roots of the trees.
        self.request_block(self.root);

        loop {
            let Some(event) = self.swarm.next().await else {
                break;
            };

            // Handle event received from the providers
            self.on_swarm_event(event).await?;

            // if no inflight queries, that means we received
            // everything requested. Finalize the blockstore.
            if self.queries.is_empty() {
                break;
            }
        }

        // NOTE(@jmg-duarte,25/02/2025): There is a way of doing this without the store,
        // but I've spent enough time with this as of now.
        // The solution is straightforward, even if not entirely simple to implement:
        // Starting from the root, we get N links to the children, those links are always sorted
        // and so, we're able to know which block goes where, as such it's just a matter of
        // building a mapping (representing the file) which just tracks the cid -> offset,size
        // and as the blocks come, seek to them and write the data, no CAR abstraction needed.

        let inner = self.blockstore.into_inner();
        let mut file = inner
            .finish_with_roots(
                self.dag
                    .values()
                    .copied()
                    .filter(|cid| !self.dag.contains_key(cid))
                    .collect::<HashSet<_>>(),
            )
            .await?;

        if self.settings.extract {
            file.rewind().await?;

            let mut extracted_file = if self.settings.overwrite {
                File::create(&self.settings.output).await?
            } else {
                File::create_new(&self.settings.output).await?
            };

            FileReader::new(file)
                .await?
                .copy_tree(&self.root, &mut extracted_file)
                .await?;
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

    #[instrument(skip_all, fields(peer_id = %peer_id, connection_id = %connection_id))]
    fn on_peer_disconnected(&mut self, peer_id: PeerId, connection_id: ConnectionId) {
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
                info!("writing block with cid: {cid:?}");
                self.blockstore.put_keyed(&cid, &data).await?;

                match cid.codec() {
                    DAG_PB_CODE => {
                        let node = <DagPbCodec as Codec<PbNode>>::decode_from_slice(&data).unwrap();

                        node.links.iter().map(|link| link.cid).for_each(|l_cid| {
                            tracing::debug!("inserting {}: {}", l_cid, cid);
                            self.dag.insert(l_cid, cid);
                            self.request_block(l_cid);
                        });
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
