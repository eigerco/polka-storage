use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    io::{Error, ErrorKind},
    time::Duration,
};

use libp2p::{
    futures::StreamExt,
    gossipsub::{self, IdentTopic},
    identify,
    identity::Keypair,
    noise, rendezvous,
    request_response::{self, Message, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use log::{debug, error, info, warn};
use primitives::p2p::{
    PeerIdRequest, PeerInfo, PeerInfoResponse, DEFAULT_REGISTRATION_TTL, GOSSIP_TOPIC,
    IDENTIFY_PROTOCOL_VERSION, REQUEST_RESPONSE_STREAM_PROTOCOL
};

use crate::service::p2p::P2PError;

#[derive(NetworkBehaviour)]
pub struct BootstrapBehaviour {
    pub rendezvous: rendezvous::server::Behaviour,
    pub identify: identify::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub request_response: request_response::cbor::Behaviour<PeerIdRequest, PeerInfoResponse>,
}

pub struct BootstrapConfig {
    address: Multiaddr,
    keypair: Keypair,
    bootstrap_addresses: Vec<Multiaddr>,
}

impl BootstrapConfig {
    pub fn new(keypair: Keypair, address: Multiaddr, bootstrap_addresses: Vec<Multiaddr>) -> Self {
        Self {
            address,
            keypair,
            bootstrap_addresses,
        }
    }

    pub fn create_swarm(
        self,
    ) -> Result<(Swarm<BootstrapBehaviour>, Multiaddr, Vec<Multiaddr>), P2PError> {
        let swarm = SwarmBuilder::with_existing_identity(self.keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| P2PError::InvalidTcpConfig)?
            .with_behaviour(|key| {
                // To content-address message, we can take the hash of message and use it as an ID.
                let message_id_fn = |message: &gossipsub::Message| {
                    let mut s = DefaultHasher::new();
                    message.data.hash(&mut s);
                    gossipsub::MessageId::from(s.finish().to_string())
                };
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .message_id_fn(message_id_fn)
                    .build()
                    .map_err(|msg| Error::new(ErrorKind::Other, msg))?;

                Ok(BootstrapBehaviour {
                    // Rendezvous server behaviour for serving new peers to connecting nodes.
                    rendezvous: rendezvous::server::Behaviour::new(
                        rendezvous::server::Config::default()
                            .with_max_ttl(DEFAULT_REGISTRATION_TTL), // Max TTL of 24 hours
                    ),
                    // The identify behaviour is used to share the external address and the public key with connecting clients.
                    identify: identify::Behaviour::new(identify::Config::new(
                        IDENTIFY_PROTOCOL_VERSION.to_string(),
                        key.public(),
                    )),
                    gossipsub: gossipsub::Behaviour::new(
                        gossipsub::MessageAuthenticity::Signed(key.clone()),
                        gossipsub_config,
                    )?,
                    request_response: request_response::cbor::Behaviour::new(
                        [(
                            StreamProtocol::new(REQUEST_RESPONSE_STREAM_PROTOCOL),
                            ProtocolSupport::Full,
                        )],
                        request_response::Config::default(),
                    ),
                })
            })
            .map_err(|_| P2PError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        Ok((swarm, self.address, self.bootstrap_addresses))
    }
}

/// Run the rendezvous point (bootstrap node).
/// Listens on the given [`Multiaddr`]
pub(crate) async fn bootstrap(
    mut swarm: Swarm<BootstrapBehaviour>,
    addr: Multiaddr,
    bootstrap_addresses: Vec<Multiaddr>,
) -> Result<(), P2PError> {
    info!("Starting P2P bootstrap node at {addr}");
    swarm.listen_on(addr)?;
    for addr in bootstrap_addresses {
        info!("Attempting to dial peer at {addr}");
        if swarm.dial(addr.clone()).is_err() {
            warn!("Failed to dial peer at address {addr}");
        }
    }
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&IdentTopic::new(GOSSIP_TOPIC))?;
    let mut registrations = HashMap::new();
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
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::Rendezvous(event)) => on_rendezvous_event(&mut swarm, event, &mut registrations),
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::Gossipsub(event)) => on_gossipsub_event(event, *swarm.local_peer_id(), &mut registrations),
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::RequestResponse(event)) => on_request_response_event(&mut swarm, event, &registrations),
                other => debug!("Encountered event: {other:?}"),
            }
        }
    }
}

/// Handles events within the rendezvous protocol
fn on_rendezvous_event(
    swarm: &mut Swarm<BootstrapBehaviour>,
    event: rendezvous::server::Event,
    registrations: &mut HashMap<PeerId, PeerInfo>,
) {
    match event {
        rendezvous::server::Event::RegistrationExpired(registration) => {
            let id = registration.record.peer_id();
            info!(
                "Registration for peer {} expired in namespace {}",
                id, registration.namespace
            );
            // Registration expired, remove entry from hashmap
            if registrations.remove(&id).is_none() {
                error!(
                    "Could not remove registration for {:?} because it was not found",
                    id
                );
            }
        }
        rendezvous::server::Event::PeerRegistered { peer, registration } => {
            info!(
                "Peer {} registered for namespace '{}' for {} seconds",
                peer, registration.namespace, registration.ttl
            );
            let peer_info = PeerInfo {
                peer_id: peer,
                multiaddrs: registration.record.addresses().to_vec(),
            };
            // Serialize PeerInfo
            let encoded_peer_info = match bincode::serialize(&peer_info) {
                Ok(info) => info,
                Err(..) => {
                    error!(peer_info:?; "Failed to serialize peer_info");
                    return;
                }
            };
            insert_or_update_registrations(registrations, peer_info);
            // Send registration information to other bootstrap nodes.
            match swarm
                .behaviour_mut()
                .gossipsub
                .publish(IdentTopic::new(GOSSIP_TOPIC), encoded_peer_info)
            {
                Ok(..) => info!("Successfully published new peer info for peer {peer}"),
                Err(e) => error!(e:?; "Failed to publish new peer info for peer {peer}"),
            }
        }
        other => debug!("Encountered other rendezvous event: {other:?}"),
    }
}

/// Handles events within the gossipsub protocol
fn on_gossipsub_event(
    event: gossipsub::Event,
    local_peer_id: PeerId,
    registrations: &mut HashMap<PeerId, PeerInfo>,
) {
    match event {
        // Received a message with peer information from another bootstrap node.
        gossipsub::Event::Message {
            propagation_source: peer_id,
            message_id: id,
            message,
        } => {
            if peer_id == local_peer_id {
                return;
            }
            let peer_info: PeerInfo = match bincode::deserialize(&message.data) {
                Ok(info) => info,
                Err(..) => {
                    error!(message:? = message.data; "Received invalid peer info from peer {peer_id:?}");
                    return;
                }
            };
            info!(
                "Got registration: {:?} with id: {} from peer: {:?}",
                peer_info, id, peer_id
            );
            insert_or_update_registrations(registrations, peer_info);
        }
        other => debug!("Encountered other gossipsub event: {other:?}"),
    }
}

/// Handles events within the request_response protocol
fn on_request_response_event(
    swarm: &mut Swarm<BootstrapBehaviour>,
    event: request_response::Event<PeerIdRequest, PeerInfoResponse>,
    registrations: &HashMap<PeerId, PeerInfo>,
) {
    match event {
        // Message received, looking up the mapping
        request_response::Event::Message { peer, message } => {
            if let Message::Request {
                request,
                channel,
                request_id,
            } = message
            {
                info!("Got request with id {request_id} from {peer}");
                let id: PeerId = request.into();
                let response = match registrations.get(&id) {
                    Some(peer_info) => {
                        info!("Peer {id:?} found in registrations");
                        PeerInfoResponse::Found(peer_info.clone())
                    }
                    None => {
                        info!("Peer {id:?} not found in registrations");
                        PeerInfoResponse::NotFound(id.into())
                    }
                };
                // Sending the peer information back to the client who opened the channel.
                // Could add retries here.
                if swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
                    .is_err()
                {
                    error!("Failed to send peer info to {peer:?}");
                }
            }
        }
        request_response::Event::OutboundFailure {
            peer,
            request_id,
            error,
        } => warn!("Failed to send message with id {request_id} to {peer}: {error}"),
        request_response::Event::InboundFailure {
            peer,
            request_id,
            error,
        } => warn!("Failed to receive message with id {request_id} from {peer}: {error}"),
        request_response::Event::ResponseSent { peer, request_id } => {
            debug!("Request with id {request_id} sent to {peer}")
        }
    }
}

/// Take the HashMap and update the entry based on peer_info.peer_id
/// or insert a new entry.
fn insert_or_update_registrations(
    registrations: &mut HashMap<PeerId, PeerInfo>,
    peer_info: PeerInfo,
) {
    registrations
        .entry(peer_info.peer_id)
        .and_modify(|info| {
            for addr in peer_info.multiaddrs.iter() {
                if !info.multiaddrs.contains(addr) {
                    info.multiaddrs.push(addr.clone())
                }
            }
        })
        .or_insert(peer_info);
}
