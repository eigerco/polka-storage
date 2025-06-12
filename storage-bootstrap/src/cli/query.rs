use libp2p::{futures::StreamExt, multiaddr::Protocol, Multiaddr, PeerId};
use primitives_p2p::PeerIdRequest;

use crate::{
    behaviour::{self, Mode},
    P2PError,
};

#[derive(Debug, Clone, clap::Parser)]
pub struct Query {
    /// List of bootstrap addresses.
    #[arg(long, value_delimiter=',', num_args=1..)]
    pub bootstrap_addresses: Vec<Multiaddr>,

    #[arg(long)]
    pub target: PeerId,

    #[arg(long)]
    pub query: PeerId,
}

impl Query {
    pub async fn run(self) -> Result<(), P2PError> {
        let mut swarm = behaviour::Behaviour::to_swarm(Mode::Query, None).await;

        for addr in self.bootstrap_addresses.iter().cloned() {
            swarm.dial(addr.clone())?;

            if let Some(Protocol::P2p(peer_id)) = addr.into_iter().last() {
                swarm.behaviour_mut().kad.add_address(&peer_id, addr);
            } else {
                tracing::warn!("Address {addr} did not have a peer id, not adding to DHT...");
                continue;
            };
        }

        loop {
            match swarm.select_next_some().await {
                libp2p::swarm::SwarmEvent::Behaviour(event) => match event {
                    crate::behaviour::BehaviourEvent::RequestResponse(event) => match event {
                        libp2p::request_response::Event::Message { message, .. } => match message {
                            libp2p::request_response::Message::Response { response, .. } => {
                                tracing::info!("Received response: {response:?}");
                                return Ok(());
                            }
                            libp2p::request_response::Message::Request { .. } => {
                                unreachable!("The query client does not handle requests")
                            }
                        },
                        event => {
                            tracing::debug!("Received unhandled request/response event: {event:?}")
                        }
                    },
                    event => tracing::debug!("Received unhandled behaviour event: {event:?}"),
                },
                libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    if peer_id != self.target {
                        tracing::trace!(
                            "Established connection to {peer_id}, not the target peer, ignoring..."
                        );
                        continue;
                    }

                    tracing::debug!(
                        "Established connection to target peer {peer_id}, sending request"
                    );
                    swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&self.target, PeerIdRequest(self.query));
                }
                event => tracing::debug!("Received unhandled event: {event:?}"),
            }
        }
    }
}
