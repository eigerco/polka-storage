use std::{sync::Arc, time::Duration};

use ::blockstore::Blockstore;
use futures::StreamExt;
use libp2p::{
    identify::{self, Event as IdentifyEvent},
    identity::Keypair,
    rendezvous::{self, client::Event as RendezvousEvent, Namespace},
    request_response::{self, Event as RequestResponseEvent, Message, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, StreamProtocol, Swarm,
};
use primitives::p2p::{
    PieceInfo, PieceInfoRequest, PieceInfoResponse, DEFAULT_REGISTRATION_TTL,
    SP_REQUEST_RESPONSE_PROTOCOL,
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

/// Unique namespace used for peer discovery and registration with rendezvous nodes.
const P2P_NAMESPACE: &str = "polka-storage";

/// The protocol version identifier string used by the identify protocol.
const IDENTIFY_PROTOCOL_VERSION: &str = "polka-storage/1.0.0";

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
    /// List of the addresses on which to listen for incoming connections.
    pub listen_on: Vec<Multiaddr>,
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
    rendezvous: rendezvous::client::Behaviour,
    bitswap: beetswap::Behaviour<MAX_MULTIHASH_LENGTH, B>,
    request_response: request_response::cbor::Behaviour<PieceInfoRequest, PieceInfoResponse>,
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
    pub fn new(args: P2pArgs<B, I>) -> Result<Self, P2pError> {
        let identify = identify::Behaviour::new(identify::Config::new(
            IDENTIFY_PROTOCOL_VERSION.to_string(),
            args.local_keypair.public(),
        ));

        let rendezvous = rendezvous::client::Behaviour::new(args.local_keypair.clone());

        let bitswap = beetswap::Behaviour::new(args.blockstore);

        let request_response = request_response::cbor::Behaviour::new(
            [(
                StreamProtocol::new(SP_REQUEST_RESPONSE_PROTOCOL),
                ProtocolSupport::Full,
            )],
            request_response::Config::default(),
        );

        let behaviour = Behaviour {
            identify,
            rendezvous,
            bitswap,
            request_response,
        };

        let mut swarm = new_swarm(args.local_keypair, behaviour)?;

        for addr in args.listen_on {
            swarm.listen_on(addr)?;
        }

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
                    // Check if there are any external addresses set, before we
                    // actually try to register ourself with the rendezvous nodes
                    if self.swarm.external_addresses().count() == 0 {
                        debug!("External address not known. Skip registration.");
                        register_interval.reset_after(Duration::from_secs(1));
                        continue;
                    }

                    // Dial rendezvous nodes again because the connection is not persisted
                    dial_rendezvous_nodes(&mut self.swarm, &self.rendezvous_nodes);

                    // Register with the nodes
                    request_registration(&mut self.swarm, &self.rendezvous_nodes);
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
                BehaviourEvent::Identify(ev) => self.on_identify_event(ev),
                BehaviourEvent::Rendezvous(ev) => self.on_rendezvous_event(ev),
                BehaviourEvent::RequestResponse(ev) => self.on_request_response_event(ev),
                _ => {}
            },
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(%address, "Listening on");
            }
            _ => {}
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn on_identify_event(&mut self, event: IdentifyEvent) {
        match event {
            // once `/identify` did its job, we know the external address of the
            // local node. The `observed_addr` is returned by the node with
            // which we identified (and they identified with us). The
            // `observed_addr` is an address that the other node observed when
            // the local node connected.
            IdentifyEvent::Received { info, .. } => {
                self.swarm.add_external_address(info.observed_addr);
            }
            _ => {}
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn on_rendezvous_event(&mut self, event: RendezvousEvent) {
        match event {
            RendezvousEvent::Registered {
                rendezvous_node,
                ttl,
                ..
            } => {
                info!(%rendezvous_node, %ttl, "Successfully registered");
            }
            RendezvousEvent::RegisterFailed {
                rendezvous_node,
                error,
                ..
            } => {
                warn!(%rendezvous_node, ?error, "Registration failed");
            }
            RendezvousEvent::Expired { peer } => {
                debug!(%peer, "Registration expired");
            }
            _ => {}
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn on_request_response_event(
        &mut self,
        event: RequestResponseEvent<PieceInfoRequest, PieceInfoResponse>,
    ) {
        match event {
            RequestResponseEvent::Message { peer, message } => {
                if let Message::Request {
                    request,
                    channel,
                    request_id,
                } = message
                {
                    trace!("Got request with id {request_id} from {peer}");

                    let response = match self.index_db.get_piece_metadata(request.piece_cid) {
                        Ok(info) => PieceInfoResponse::Found(PieceInfo { roots: info.roots }),
                        Err(err) => {
                            error!(?err, "error occurred while retrieving piece info");
                            PieceInfoResponse::NotFound(request.piece_cid)
                        }
                    };

                    if self
                        .swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, response)
                        .is_err()
                    {
                        error!("Failed to send piece info to {peer:?}");
                    }
                }
            }
            RequestResponseEvent::OutboundFailure {
                peer,
                request_id,
                error,
            } => warn!("Failed to send response with id {request_id} to {peer}: {error}"),
            RequestResponseEvent::InboundFailure {
                peer,
                request_id,
                error,
            } => warn!("Failed to receive message with id {request_id} from {peer}: {error}"),
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                trace!("Response with id {request_id} sent to {peer}")
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
                warn!(?err, %rendezvous_peer, %rendezvous_addr, "rendezvous node dialing error");
                continue;
            }
        }
    }
}

/// Request registration with the rendezvous nodes.
fn request_registration<B>(swarm: &mut Swarm<Behaviour<B>>, nodes: &[(PeerId, Multiaddr)])
where
    B: Blockstore,
{
    for (rendezvous_peer, rendezvous_addr) in nodes {
        // Register with the node
        let request_result = swarm.behaviour_mut().rendezvous.register(
            Namespace::from_static(P2P_NAMESPACE),
            *rendezvous_peer,
            Some(REGISTRATION_TTL.as_secs()),
        );

        match request_result {
            Ok(_) => {
                info!(%rendezvous_peer, %rendezvous_addr, "Registration requested");
            }
            Err(err) => {
                warn!(?err, %rendezvous_peer, %rendezvous_addr, "Registration request failed");
            }
        }
    }
}
