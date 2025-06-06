use libp2p::{futures::StreamExt, multiaddr::Protocol, Multiaddr, PeerId};
use primitives_p2p::{PeerIdRequest, PeerInfoResponse};

use crate::{swarm::create_swarm, P2PError};

#[derive(Debug, Clone, clap::Parser)]
pub struct QueryConfig {
    /// List of bootstrap addresses.
    #[arg(long, value_delimiter=',', num_args=1..)]
    pub bootstrap_addresses: Vec<Multiaddr>,

    #[arg(long)]
    pub query: PeerId,
}

impl QueryConfig {
    pub async fn run(self) -> Result<(), P2PError> {
        let mut swarm = create_swarm(None).await?;

        for addr in self.bootstrap_addresses.iter().cloned() {
            swarm.dial(addr)?;
        }

        for addr in self.bootstrap_addresses {
            let Some(Protocol::P2p(peer_id)) = addr.into_iter().last() else {
                tracing::warn!("Address {addr} did not have a peer id, skipping...");
                continue;
            };
            swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, PeerIdRequest(self.query));
        }

        loop {
            match swarm.select_next_some().await {
                libp2p::swarm::SwarmEvent::Behaviour(event) => match event {
                    crate::bootstrap::BootstrapBehaviourEvent::RequestResponse(event) => {
                        match event {
                            libp2p::request_response::Event::Message { peer, message } => {
                                match message {
                                    libp2p::request_response::Message::Response {
                                        request_id,
                                        response,
                                    } => {
                                        tracing::info!("Received response: {response:?}");
                                        return Ok(());
                                    }
                                    libp2p::request_response::Message::Request { .. } => {
                                        unreachable!("The query client does not handle requests")
                                    }
                                }
                            }
                            event => tracing::debug!(
                                "Received unhandled request/response event: {event:?}"
                            ),
                        }
                    }
                    event => tracing::debug!("Received unhandled behaviour event: {event:?}"),
                },
                event => tracing::debug!("Received unhandled event: {event:?}"),
            }
        }
    }
}
