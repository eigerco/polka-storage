use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use beetswap::{Event, QueryId};
use blockstore::Blockstore;
use cid::Cid;
use futures::StreamExt;
use ipld_core::codec::Codec;
use ipld_dagpb::{DagPbCodec, PbNode};
use libp2p::{
    noise,
    request_response::{self, Message, ProtocolSupport},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use libp2p_core::ConnectedPoint;
use libp2p_swarm::{ConnectionId, DialError, NetworkBehaviour, SwarmEvent};
use mater::{blockstore::ReadWriteBlockstore, FileReader, DAG_PB_CODE, RAW_CODE};
use primitives::p2p::{PieceInfoRequest, PieceInfoResponse, SP_REQUEST_RESPONSE_PROTOCOL};
use thiserror::Error;
use tokio::{
    fs::{File, OpenOptions},
    io::AsyncSeekExt,
};
use tracing::{debug, error, info, instrument, trace};

const MAX_MULTIHASH_LENGTH: usize = 64;

/// Errors that can occur while retrieving some content.
#[derive(Debug, Error)]
pub enum DownloadError {
    /// When an unknown piece is requested
    #[error("Unknown piece")]
    PieceUnknown,
    /// Is returned for the car archives that have unsupported number of roots.
    #[error("Unsupported number of roots")]
    UnsupportedNumRoots,
    /// Failed to initialize noise protocol.
    #[error("Failed to initialize noise: {0}")]
    Noise(#[from] noise::Error),
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

/// Behaviour used by the download client.
#[derive(NetworkBehaviour)]
pub struct Behaviour<B>
where
    B: Blockstore + 'static,
{
    pub request_response: request_response::cbor::Behaviour<PieceInfoRequest, PieceInfoResponse>,
    pub bitswap: beetswap::Behaviour<MAX_MULTIHASH_LENGTH, B>,
}

pub struct DownloadClientSettings {
    output: PathBuf,
    extract: bool,
    overwrite: bool,
}

impl DownloadClientSettings {
    pub fn new(output: PathBuf, extract: bool, overwrite: bool) -> Self {
        Self {
            output,
            extract,
            overwrite,
        }
    }
}

/// A Downloadclient is used to download blocks from the storage provider. Single client
/// supports getting a single payload.
pub struct DownloadClient {
    settings: DownloadClientSettings,

    /// Providers of data
    providers: Vec<(PeerId, Multiaddr)>,
    /// Swarm instance
    swarm: Swarm<Behaviour<PassthroughBlockstore>>,
    /// The in flight block queries. If empty we know that the client received
    /// all requested data.
    queries: HashMap<QueryId, Cid>,
    /// Blockstore used by the client to store blocks into.
    blockstore: ReadWriteBlockstore<File>,
    /// The root is set when we receive a piece_cid to payload_cid mapping from
    /// a storage provider. It is only set once.
    root: Option<Cid>,
    /// CAR block DAG mapping children to parents. (The A in DAG isn't checked!)
    dag: HashMap<Cid, Cid>,
}

impl DownloadClient {
    pub async fn new(
        providers: Vec<(PeerId, Multiaddr)>,
        settings: DownloadClientSettings,
    ) -> Result<Self, DownloadError> {
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
            root: None,
            dag: HashMap::new(),
        })
    }

    /// Start download of some content with a payload cid.
    pub async fn download(mut self, &piece_cid: &Cid) -> Result<(), DownloadError> {
        // Dial all providers
        for (peer, multiaddr) in self.providers.clone() {
            self.swarm.add_peer_address(peer, multiaddr);

            // Request a piece info if no root known
            if self.root.is_none() {
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer, PieceInfoRequest { piece_cid });

                info!(%peer, %piece_cid, "requested piece info");
            }
        }

        loop {
            let Some(event) = self.swarm.next().await else {
                break;
            };

            // Handle event received from the providers
            self.on_swarm_event(event).await?;

            // if no inflight queries and the root is set, that means we
            // received everything requested. We are checking if root is set
            // because the root is None until we receive a response mapping
            // piece_cid -> root_cid from the storage provider. The queries are
            // also empty until we have a root set. That is because, without a
            // root we don't know the block to start the data download with.
            if self.root.is_some() && self.queries.is_empty() {
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
            let Some(root) = &self.root else {
                return Err(DownloadError::UnsupportedNumRoots);
            };

            file.rewind().await?;

            let mut extracted_file = if self.settings.overwrite {
                File::create(&self.settings.output).await?
            } else {
                File::create_new(&self.settings.output).await?
            };

            FileReader::new(file)
                .await?
                .copy_tree(&root, &mut extracted_file)
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
    ) -> Result<(), DownloadError> {
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
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(event)) => {
                self.on_request_response(event)?;
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
    fn on_request_response(
        &mut self,
        event: request_response::Event<PieceInfoRequest, PieceInfoResponse>,
    ) -> Result<(), DownloadError> {
        if let request_response::Event::Message { peer, message } = event {
            if let Message::Response { response, .. } = message {
                match response {
                    PieceInfoResponse::Found(piece_info) => {
                        // Root is already known.
                        if self.root.is_some() {
                            return Ok(());
                        }

                        tracing::info!(%peer, "Received piece info from peer");

                        let root = if piece_info.roots.len() == 1 {
                            Ok(piece_info.roots[0])
                        } else {
                            Err(DownloadError::UnsupportedNumRoots)
                        }?;

                        // Start requesting blocks
                        self.root = Some(root);
                        self.request_block(root);
                    }
                    PieceInfoResponse::NotFound(_) => return Err(DownloadError::PieceUnknown),
                }
            }
        }

        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    async fn on_bitswap_event(&mut self, event: Event) -> Result<(), DownloadError> {
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

/// Initialize a new swarm with our custom Behaviour.
fn new_swarm<B>(blockstore: Arc<B>) -> Result<Swarm<Behaviour<B>>, DownloadError>
where
    B: Blockstore + 'static,
{
    let bitswap = beetswap::Behaviour::new(blockstore);
    let request_response = request_response::cbor::Behaviour::new(
        [(
            StreamProtocol::new(SP_REQUEST_RESPONSE_PROTOCOL),
            ProtocolSupport::Full,
        )],
        request_response::Config::default(),
    );

    let behaviour = Behaviour {
        request_response,
        bitswap,
    };

    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| behaviour)
        .expect("Moving behaviour doesn't fail")
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}
