use std::{sync::Arc, time::Duration};

use ::blockstore::Blockstore;
use futures::StreamExt;
use libp2p::{
    identify::{self, Event as IdentifyEvent},
    identity::Keypair,
    request_response::{self, Event as RequestResponseEvent, Message, ProtocolSupport},
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        NetworkBehaviour, SwarmEvent,
    },
    Multiaddr, PeerId, StreamProtocol, Swarm,
};
use libp2p_length_prefix_codec::LpCbor;
use primitives_p2p::{
    PieceInfo, PieceInfoRequest, PieceInfoResponse, DEFAULT_REGISTRATION_TTL,
    IDENTIFY_PROTOCOL_VERSION, SP_REQUEST_RESPONSE_PROTOCOL,
};
use swarm::new_swarm;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

pub mod blockstore;
mod error;
mod swarm;

pub use error::P2pError;

use crate::indexer::local_index_directory::Service;

/// The time-to-live duration for node registration with rendezvous points.
const REGISTRATION_TTL: Duration = Duration::from_secs(DEFAULT_REGISTRATION_TTL);

/// Maximum length allowed for a multihash in bytes.
const MAX_MULTIHASH_LENGTH: usize = 64;

/// Arguments used to configure the [`P2p`].
pub struct P2pArgs<B, I>
where
    B: Blockstore,
    I: Service,
{
    /// The keypair to be used as the identity.
    pub local_keypair: Keypair,
    /// List of rendezvous nodes to register to.
    pub rendezvous_nodes: Vec<(PeerId, Multiaddr)>,
    /// P2P tcp listen address
    pub p2p_tcp_listen_address: Multiaddr,
    /// P2P ws listen address
    pub p2p_ws_listen_address: Multiaddr,
    /// The blockstore used for content retrieval.
    pub blockstore: Arc<B>,
    /// Piece index database.
    pub index_db: Arc<I>,
}

/// Our network behaviour.
#[derive(NetworkBehaviour)]
pub struct Behaviour<B>
where
    B: Blockstore + 'static,
{
    identify: identify::Behaviour,
    bitswap: beetswap::Behaviour<MAX_MULTIHASH_LENGTH, B>,
    request_response: request_response::Behaviour<LpCbor<PieceInfoRequest, PieceInfoResponse>>,
}

/// Worker manages the P2P networking lifecycle and peer interactions.
///
/// The typical network flow is:
/// 1. Start listening for connections
/// 2. Connect to configured rendezvous nodes
/// 3. Exchange identity information
/// 4. Register our presence with the rendezvous nodes (repeated periodically)
/// 5. Handle incoming bitswap requests
pub struct Worker<B, I>
where
    B: Blockstore + 'static,
    I: Service + 'static,
{
    swarm: Swarm<Behaviour<B>>,
    rendezvous_nodes: Vec<(PeerId, Multiaddr)>,
    index_db: Arc<I>,
}

impl<B, I> Worker<B, I>
where
    B: Blockstore,
    I: Service,
{
    pub async fn new(args: P2pArgs<B, I>) -> Result<Self, P2pError> {
        let identify = identify::Behaviour::new(identify::Config::new(
            IDENTIFY_PROTOCOL_VERSION.to_string(),
            args.local_keypair.public(),
        ));

        let bitswap = beetswap::Behaviour::new(args.blockstore);

        let request_response =
            request_response::Behaviour::<LpCbor<PieceInfoRequest, PieceInfoResponse>>::new(
                [(
                    StreamProtocol::new(SP_REQUEST_RESPONSE_PROTOCOL),
                    ProtocolSupport::Full,
                )],
                request_response::Config::default(),
            );

        let behaviour = Behaviour {
            identify,
            bitswap,
            request_response,
        };

        let mut swarm = new_swarm(args.local_keypair, behaviour).await?;
        swarm.listen_on(args.p2p_tcp_listen_address)?;
        swarm.listen_on(args.p2p_ws_listen_address)?;

        // We are dialing the rendezvous nodes. After the connection is
        // successfully established, the identify message received from the
        // nodes tells us our public multiaddr which we'll register.
        dial_rendezvous_nodes(&mut swarm, &args.rendezvous_nodes);

        Ok(Worker {
            swarm,
            rendezvous_nodes: args.rendezvous_nodes,
            index_db: args.index_db,
        })
    }

    pub async fn run(mut self, cancellation_token: CancellationToken) -> Result<(), P2pError> {
        let mut register_interval = tokio::time::interval(REGISTRATION_TTL);

        loop {
            select! {
                _ = register_interval.tick() => {
                    // Dial rendezvous nodes again because the connection is not persisted
                    // On dialing the identify protocol *should* be exchanged which will trigger
                    // a "re-register" in the Kadmelia server, adding it to the server's kbucket
                    dial_rendezvous_nodes(&mut self.swarm, &self.rendezvous_nodes);
                }
                event = self.swarm.select_next_some() => self.on_swarm_event(event),
                _ = cancellation_token.cancelled() => {
                    info!("P2P worker received shutdown signal");
                    return Ok(());
                }
            }
        }
    }

    fn on_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent<B>>) {
        match event {
            SwarmEvent::Behaviour(ev) => match ev {
                BehaviourEvent::RequestResponse(ev) => self.on_request_response_event(ev),
                _ => tracing::trace!("Unhandled behaviour event: {ev:?}"),
            },
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(%address, "Listening on");
            }
            _ => tracing::trace!("Unhandled swarm event: {event:?}"),
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn on_request_response_event(
        &mut self,
        event: RequestResponseEvent<PieceInfoRequest, PieceInfoResponse>,
    ) {
        match event {
            RequestResponseEvent::Message {
                peer,
                message:
                    Message::Request {
                        request,
                        channel,
                        request_id,
                    },
            } => {
                debug!("Got request with id {request_id} from {peer}");

                let response = match self.index_db.get_piece_metadata(request.piece_cid) {
                    Ok(info) => PieceInfoResponse::Found(PieceInfo { roots: info.roots }),
                    Err(err) => {
                        error!(?err, "error occurred while retrieving piece info");
                        PieceInfoResponse::NotFound(request.piece_cid)
                    }
                };

                let response_result = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response);
                if let Err(err) = response_result {
                    error!(%peer, ?err, "Failed to respond with piece info");
                }
            }
            RequestResponseEvent::OutboundFailure {
                peer,
                request_id,
                error,
            } => tracing::error!("Failed to send response with id {request_id} to {peer}: {error}"),
            RequestResponseEvent::InboundFailure {
                peer,
                request_id,
                error,
            } => tracing::error!(
                "Failed to receive message with id {request_id} from {peer}: {error}"
            ),
            RequestResponseEvent::ResponseSent { .. } | RequestResponseEvent::Message { .. } => {
                tracing::trace!("Unhandled request response event: {event:?}")
            }
        }
    }
}

/// Dials rendezvous nodes.
fn dial_rendezvous_nodes<B>(swarm: &mut Swarm<Behaviour<B>>, nodes: &[(PeerId, Multiaddr)])
where
    B: Blockstore,
{
    for (rendezvous_peer, rendezvous_addr) in nodes {
        // Start dialing the node if needed
        if !swarm.is_connected(rendezvous_peer) {
            if let Err(err) = swarm.dial(rendezvous_addr.clone()) {
                warn!(%rendezvous_peer, %rendezvous_addr, "Failed to dial rendezvous node with error: {err}");
                continue;
            }
        }
    }
}
