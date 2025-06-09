use std::time::Duration;

use libp2p::{
    futures::StreamExt,
    identify,
    identity::Keypair,
    kad, noise,
    request_response::{self, Message, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, StreamProtocol, Swarm, SwarmBuilder,
};
use libp2p_length_prefix_codec::LpCbor;
use primitives_p2p::{
    keypair_value_parser, PeerIdRequest, PeerInfo, PeerInfoResponse,
    BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL, IDENTIFY_PROTOCOL_VERSION,
};
use tracing::{debug, error, info, warn};

use crate::{
    behaviour::{self, Behaviour, Mode},
    P2PError,
};

#[derive(Debug, Clone, clap::Parser)]
pub struct Bootstrap {
    /// Listening addresses.
    #[arg(long, value_delimiter=',', num_args=1..)]
    pub listen_addresses: Vec<Multiaddr>,

    /// Public addresses — will be automatically added to the swarm as external addresses.
    #[arg(long, value_delimiter=',', num_args=1..)]
    pub public_addresses: Vec<Multiaddr>,

    /// List of bootstrap addresses.
    #[arg(long, value_delimiter=',', num_args=1..)]
    pub bootstrap_addresses: Vec<Multiaddr>,

    /// P2P identity keypair (private ED25519 key), if missing a random one will be used.
    #[arg(long, value_parser = keypair_value_parser)]
    pub keypair: Option<Keypair>,
}

impl Bootstrap {
    pub async fn run(self) -> Result<(), P2PError> {
        let swarm = behaviour::Behaviour::to_swarm(Mode::Server, self.keypair).await;
        bootstrap(
            swarm,
            self.listen_addresses,
            self.public_addresses,
            self.bootstrap_addresses,
        )
        .await
    }
}

/// Run the rendezvous point (bootstrap node).
/// Listens on the given [`Multiaddr`]
pub(crate) async fn bootstrap(
    mut swarm: Swarm<Behaviour>,
    listen_addresses: Vec<Multiaddr>,
    public_addresses: Vec<Multiaddr>,
    bootstrap_addresses: Vec<Multiaddr>,
) -> Result<(), P2PError> {
    // Kad sets this when the external address get's confirmed
    // but this case is always a server
    swarm.behaviour_mut().kad.set_mode(Some(kad::Mode::Server));

    for addr in listen_addresses {
        info!("P2P bootstrap: listening at {addr}");
        swarm.listen_on(addr)?;
    }

    for addr in public_addresses {
        swarm.add_external_address(addr);
    }

    for addr in bootstrap_addresses {
        info!("Attempting to dial peer at {addr}");
        match swarm.dial(addr.clone()) {
            Ok(()) => info!("Successfully dialed {addr}"),
            Err(err) => error!("Failed to dial peer at address {addr} with error: {err}"),
        }
    }

    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("Listening on {}", address);
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    info!("Connected to {}", peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    info!("Disconnected from {}", peer_id);
                }
                SwarmEvent::Behaviour(crate::behaviour::BehaviourEvent::Identify(event)) => {
                    match event {
                        identify::Event::Received {  peer_id, info, .. } => {
                            tracing::trace!("Received an identify received event");
                            for addr in info.listen_addrs {
                                tracing::debug!("Adding address to kademlia: peer_id={peer_id}, addr={addr:?}");
                                swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                            }
                        },
                        _ => tracing::debug!("Unhandled Identify event: {event:?}"),
                    };
                }
                SwarmEvent::Behaviour(crate::behaviour::BehaviourEvent::RequestResponse(event)) => on_request_response_event(&mut swarm, event, ),
                other => debug!("Encountered event: {other:?}"),
            }
        }
    }
}

/// Handles events within the request_response protocol
fn on_request_response_event(
    swarm: &mut Swarm<Behaviour>,
    event: request_response::Event<PeerIdRequest, PeerInfoResponse>,
) {
    match event {
        // Message received, looking up the mapping
        request_response::Event::Message { message, .. } => {
            if let Message::Request {
                request,
                channel,
                request_id,
            } = message
            {
                tracing::trace!("Received a request-response request ({request_id}): {request:?}");
                let Some(kref) = swarm.behaviour_mut().kad.kbucket(request.0) else {
                    tracing::trace!(
                        "KBucket query returned None, we're the node containing the peer"
                    );
                    let peer_id = *swarm.local_peer_id();
                    let multiaddrs = swarm.external_addresses().cloned().collect();
                    let peer_info = PeerInfoResponse::Found(PeerInfo {
                        peer_id,
                        multiaddrs,
                    });
                    tracing::trace!("Sending response to request ({request_id}): {peer_info:?}");

                    if swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, peer_info)
                        .is_err()
                    {
                        tracing::error!("Failed to send response to request {request_id}");
                    }
                    return;
                };

                tracing::trace!("KBucket contains peer {}, searching for it", request.0);
                let entry = kref
                    .iter()
                    .find(|entry| entry.node.key.preimage() == &request.0);
                let Some(entry) = entry else {
                    tracing::trace!("Could not find peer {} in KBucket", request.0);
                    if swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, PeerInfoResponse::NotFound(request))
                        .is_err()
                    {
                        tracing::error!("Failed to send response to request {request_id}")
                    }
                    return;
                };

                let peer_id = *entry.node.key.preimage();
                let multiaddrs = entry.node.value.clone().into_vec();
                let response = PeerInfoResponse::Found(PeerInfo {
                    peer_id,
                    multiaddrs,
                });
                tracing::trace!("Found entry in KBucket, sending response: {response:?}");
                if swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
                    .is_err()
                {
                    tracing::error!("Failed to send response to request {request_id}");
                }
            }
        }
        request_response::Event::OutboundFailure {
            peer,
            request_id,
            error,
        } => warn!("Failed to send response with id {request_id} to {peer}: {error}"),
        request_response::Event::InboundFailure {
            peer,
            request_id,
            error,
        } => warn!("Failed to receive message with id {request_id} from {peer}: {error}"),
        request_response::Event::ResponseSent { peer, request_id } => {
            debug!("Response with id {request_id} sent to {peer}")
        }
    }
}
