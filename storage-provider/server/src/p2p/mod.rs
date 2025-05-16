use std::{sync::Arc, time::Duration};

use ::blockstore::Blockstore;
use futures::StreamExt;
use libp2p::{
    identify::{self},
    identity::Keypair,
    request_response::{
        self, Event as RequestResponseEvent, InboundRequestId, Message, ProtocolSupport,
        ResponseChannel,
    },
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, StreamProtocol, Swarm,
};
use libp2p_length_prefix_codec::LpCbor;
use primitives_p2p::{
    services::{self, Services},
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
    pub p2p_listen_addresses: Vec<Multiaddr>,
    pub p2p_external_addresses: Vec<Multiaddr>,
    /// The blockstore used for content retrieval.
    pub blockstore: Arc<B>,
    /// Piece index database.
    pub index_db: Arc<I>,

    pub services: Services,
}

/// Our network behaviour.
#[derive(NetworkBehaviour)]
pub struct Behaviour<B>
where
    B: Blockstore + 'static,
{
    identify: identify::Behaviour,
    bitswap: beetswap::Behaviour<MAX_MULTIHASH_LENGTH, B>,
    piece_info_rr: request_response::Behaviour<LpCbor<PieceInfoRequest, PieceInfoResponse>>,
    services_rr: request_response::Behaviour<LpCbor<services::Request, services::Response>>,
}

fn setup_piece_info_rr() -> request_response::Behaviour<LpCbor<PieceInfoRequest, PieceInfoResponse>>
{
    request_response::Behaviour::<LpCbor<PieceInfoRequest, PieceInfoResponse>>::new(
        [(
            StreamProtocol::new(SP_REQUEST_RESPONSE_PROTOCOL),
            ProtocolSupport::Full,
        )],
        request_response::Config::default(),
    )
}

fn setup_services_rr() -> request_response::Behaviour<LpCbor<services::Request, services::Response>>
{
    request_response::Behaviour::<LpCbor<services::Request, services::Response>>::new(
        [(
            StreamProtocol::new(services::PROTOCOL_NAME),
            ProtocolSupport::Inbound,
        )],
        request_response::Config::default(),
    )
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
    services: services::Services,
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

        let behaviour = Behaviour {
            identify,
            bitswap,
            piece_info_rr: setup_piece_info_rr(),
            services_rr: setup_services_rr(),
        };

        let mut swarm = new_swarm(args.local_keypair, behaviour).await?;
        for listen_addr in args.p2p_listen_addresses.into_iter() {
            swarm.listen_on(listen_addr)?;
        }

        for external_addr in args.p2p_external_addresses.into_iter() {
            swarm.add_external_address(external_addr);
        }

        // We are dialing the rendezvous nodes. After the connection is
        // successfully established, the identify message received from the
        // nodes tells us our public multiaddr which we'll register.
        dial_rendezvous_nodes(&mut swarm, &args.rendezvous_nodes);

        Ok(Worker {
            swarm,
            rendezvous_nodes: args.rendezvous_nodes,
            index_db: args.index_db,
            services: args.services,
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
                BehaviourEvent::PieceInfoRr(ev) => self.on_piece_info_rr_event(ev),
                BehaviourEvent::ServicesRr(event) => self.on_services_rr_event(event),
                _ => tracing::trace!("Unhandled behaviour event: {ev:?}"),
            },
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(%address, "Listening on");
            }
            _ => tracing::trace!("Unhandled swarm event: {event:?}"),
        }
    }

    #[instrument(level = "info", skip(self))]
    fn on_piece_info_rr_event(
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
            } => self.on_piece_info_request(peer, request, channel, request_id),
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

    fn on_piece_info_request(
        &mut self,
        peer: PeerId,
        request: PieceInfoRequest,
        channel: ResponseChannel<PieceInfoResponse>,
        request_id: InboundRequestId,
    ) {
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
            .piece_info_rr
            .send_response(channel, response);
        if let Err(err) = response_result {
            error!(%peer, ?err, "Failed to respond with piece info");
        }
    }

    #[instrument(skip(self))]
    fn on_services_rr_event(
        &mut self,
        event: request_response::Event<services::Request, services::Response>,
    ) {
        match event {
            RequestResponseEvent::Message {
                peer,
                message:
                    request_response::Message::Request {
                        request_id,
                        request,
                        channel,
                    },
            } => self.on_services_rr_request(peer, request, channel, request_id),
            RequestResponseEvent::InboundFailure { .. } => {
                tracing::error!("Inbound request failed with error: {event:?}")
            }
            RequestResponseEvent::ResponseSent { .. } => {
                tracing::trace!("Unhandled event: {event:?}")
            }
            RequestResponseEvent::Message {
                message: request_response::Message::Response { .. },
                ..
            }
            | RequestResponseEvent::OutboundFailure { .. } => {
                unreachable!("Protocol is configured as inbound only!")
            }
        }
    }

    fn on_services_rr_request(
        &mut self,
        peer: PeerId,
        request: services::Request,
        channel: ResponseChannel<services::Response>,
        request_id: InboundRequestId,
    ) {
        tracing::trace!("Received request from {peer}: {request:?}");
        let services = match request {
            services::Request::All => self.services.clone(),
        };
        if self
            .swarm
            .behaviour_mut()
            .services_rr
            .send_response(channel, services::Response { services })
            .is_err()
        {
            tracing::error!("Failed to send a response to request {request_id}");
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
