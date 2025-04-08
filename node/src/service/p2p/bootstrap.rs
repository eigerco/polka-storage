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
use log::{debug, error, info, warn};
use primitives_p2p::{
    PeerIdRequest, PeerInfo, PeerInfoResponse, BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL,
    IDENTIFY_PROTOCOL_VERSION,
};

use crate::service::p2p::P2PError;

#[derive(NetworkBehaviour)]
pub struct BootstrapBehaviour {
    pub identify: identify::Behaviour,
    pub request_response: request_response::Behaviour<LpCbor<PeerIdRequest, PeerInfoResponse>>,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
}

pub struct BootstrapConfig {
    tcp_address: Multiaddr,
    websocket_address: Multiaddr,
    keypair: Keypair,
    bootstrap_addresses: Vec<Multiaddr>,
}

impl BootstrapConfig {
    pub fn new(
        keypair: Keypair,
        tcp_address: Multiaddr,
        websocket_address: Multiaddr,
        bootstrap_addresses: Vec<Multiaddr>,
    ) -> Self {
        Self {
            tcp_address,
            websocket_address,
            keypair,
            bootstrap_addresses,
        }
    }

    pub async fn create_swarm(
        self,
    ) -> Result<
        (
            Swarm<BootstrapBehaviour>,
            Multiaddr,
            Multiaddr,
            Vec<Multiaddr>,
        ),
        P2PError,
    > {
        let swarm = SwarmBuilder::with_existing_identity(self.keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| P2PError::InvalidTcpConfig)?
            .with_websocket(noise::Config::new, yamux::Config::default)
            .await
            .map_err(|_| P2PError::InvalidWebsocketConfig)?
            .with_behaviour(|key| {
                Ok(BootstrapBehaviour {
                    // The identify behaviour is used to share the external address and the public key with connecting clients.
                    identify: identify::Behaviour::new(identify::Config::new(
                        IDENTIFY_PROTOCOL_VERSION.to_string(),
                        key.public(),
                    )),
                    request_response: request_response::Behaviour::new(
                        [(
                            StreamProtocol::new(BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL),
                            ProtocolSupport::Full,
                        )],
                        request_response::Config::default(),
                    ),
                    kad: kad::Behaviour::new(
                        key.public().to_peer_id(),
                        kad::store::MemoryStore::new(key.public().to_peer_id()),
                    ),
                })
            })
            .map_err(|_| P2PError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        Ok((
            swarm,
            self.tcp_address,
            self.websocket_address,
            self.bootstrap_addresses,
        ))
    }
}

/// Run the rendezvous point (bootstrap node).
/// Listens on the given [`Multiaddr`]
pub(crate) async fn bootstrap(
    mut swarm: Swarm<BootstrapBehaviour>,
    tcp_addr: Multiaddr,
    ws_addr: Multiaddr,
    bootstrap_addresses: Vec<Multiaddr>,
) -> Result<(), P2PError> {
    info!("Starting P2P bootstrap node at {tcp_addr}");

    swarm.listen_on(tcp_addr)?;
    swarm.listen_on(ws_addr)?;

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
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::Identify(event)) => {
                    match event {
                        identify::Event::Received {  peer_id, info, .. } => {
                            log::trace!("Received an identify received event");
                            for addr in info.listen_addrs {
                                log::debug!("Adding address to kademlia: peer_id={peer_id}, addr={addr:?}");
                                swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                            }
                        },
                        _ => log::debug!("Unhandled Identify event: {event:?}"),
                    };
                }
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::RequestResponse(event)) => on_request_response_event(&mut swarm, event, ),
                other => debug!("Encountered event: {other:?}"),
            }
        }
    }
}

/// Handles events within the request_response protocol
fn on_request_response_event(
    swarm: &mut Swarm<BootstrapBehaviour>,
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
                log::trace!("Received a request-response request ({request_id}): {request:?}");
                // When we receive a message from
                let Some(kref) = swarm.behaviour_mut().kad.kbucket(request.0) else {
                    log::trace!("KBucket query returned None, we're the node containing the peer");
                    let peer_id = swarm.local_peer_id().clone();
                    let multiaddrs = swarm.external_addresses().cloned().collect();
                    let peer_info = PeerInfoResponse::Found(PeerInfo {
                        peer_id,
                        multiaddrs,
                    });
                    log::trace!("Sending response to request ({request_id}): {peer_info:?}");
                    swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, peer_info)
                        .unwrap(); // TODO: remove
                    return;
                };

                log::trace!("KBucket contains peer {}, searching for it", request.0);
                let entry = kref
                    .iter()
                    .filter(|entry| entry.node.key.preimage() == &request.0)
                    .take(1)
                    .collect::<Vec<_>>()
                    .pop();
                let Some(entry) = entry else {
                    log::trace!("Could not find peer {} in KBucket", request.0);
                    swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, PeerInfoResponse::NotFound(request))
                        .unwrap();
                    return;
                };
                let peer_id = entry.node.key.preimage().clone();
                let multiaddrs = entry.node.value.clone().into_vec();
                let response = PeerInfoResponse::Found(PeerInfo {
                    peer_id,
                    multiaddrs,
                });
                log::trace!("Found entry in KBucket, sending response: {response:?}");
                swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
                    .unwrap();
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
