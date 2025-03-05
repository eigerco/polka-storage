use std::time::Duration;

use anyhow::{anyhow, bail};
use libp2p::{
    futures::StreamExt,
    noise,
    request_response::{self, Message, ProtocolSupport},
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use primitives::p2p::{PeerIdRequest, PeerInfoResponse, BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL};
use tracing::info;

/// Get multiaddresses used by the peer.
pub async fn find_multiaddr_storage_provider(
    bootstrap_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    sp_peer_id: PeerId,
) -> Result<Vec<Multiaddr>, anyhow::Error> {
    let mut swarm = create_swarm()?;

    // Add known external peer to the swarm
    swarm.add_peer_address(bootstrap_peer_id.clone(), bootstrap_address.clone());

    // Request the info about the peer from the bootstrap node
    swarm
        .behaviour_mut()
        .send_request(&bootstrap_peer_id, sp_peer_id.into());

    // Wait for the response
    let peer_info = wait_peer_info_response(swarm).await?;

    match peer_info {
        PeerInfoResponse::NotFound(peer) => Err(anyhow!("{:?} is not registered", peer)),
        PeerInfoResponse::Found(peer_info) => Ok(peer_info.multiaddrs),
    }
}

/// Pull the swarm until we receive peer info response or an error is observed.
async fn wait_peer_info_response(
    mut swarm: Swarm<request_response::cbor::Behaviour<PeerIdRequest, PeerInfoResponse>>,
) -> Result<PeerInfoResponse, anyhow::Error> {
    loop {
        let Some(event) = swarm.next().await else {
            bail!("Swarm was exhausted");
        };

        if let SwarmEvent::Behaviour(event) = event {
            match event {
                request_response::Event::Message { message, .. } => {
                    if let Message::Response { response, .. } = message {
                        info!(?response, "Received peer info response");
                        return Ok(response);
                    }
                }
                request_response::Event::OutboundFailure { error, .. } => {
                    // This happens if we couldn't request the storage provider
                    // info from the bootstrap node
                    bail!(error)
                }
                _ => {}
            }
        }
    }
}

fn create_swarm(
) -> Result<Swarm<request_response::cbor::Behaviour<PeerIdRequest, PeerInfoResponse>>, anyhow::Error>
{
    Ok(SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| {
            request_response::cbor::Behaviour::new(
                [(
                    StreamProtocol::new(BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL),
                    ProtocolSupport::Full,
                )],
                request_response::Config::default(),
            )
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
        .build())
}
