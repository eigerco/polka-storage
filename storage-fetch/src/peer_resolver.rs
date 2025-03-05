use std::time::Duration;

use anyhow::bail;
use libp2p::{
    futures::StreamExt,
    noise,
    request_response::{self, Message, ProtocolSupport},
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use primitives::p2p::{PeerIdRequest, PeerInfoResponse, REQUEST_RESPONSE_STREAM_PROTOCOL};
use tracing::{info, warn};

pub async fn find_multiaddr_storage_provider(
    bootstrap_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    sp_peer_id: PeerId,
) -> Result<Vec<Multiaddr>, anyhow::Error> {
    let swarm = create_swarm()?;

    let peer_info = run_discover(swarm, bootstrap_address, &bootstrap_peer_id, sp_peer_id)
        .await
        .unwrap();

    match peer_info {
        PeerInfoResponse::NotFound(peer) => {
            warn!(?peer, "peer is not registered");
            bail!("{:?} is not registered", peer)
        }
        PeerInfoResponse::Found(peer_info) => {
            info!(?peer_info, "received storage provider info");
            Ok(peer_info.multiaddrs)
        }
    }
}

/// Create a discovery swarm
fn create_swarm(
) -> Result<Swarm<request_response::cbor::Behaviour<PeerIdRequest, PeerInfoResponse>>, anyhow::Error>
{
    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| {
            request_response::cbor::Behaviour::new(
                [(
                    StreamProtocol::new(REQUEST_RESPONSE_STREAM_PROTOCOL),
                    ProtocolSupport::Full,
                )],
                request_response::Config::default(),
            )
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
        .build();
    Ok(swarm)
}

/// Run the discovery swarm and request the peer ID to multiaddrs mapping.
async fn run_discover(
    mut swarm: Swarm<request_response::cbor::Behaviour<PeerIdRequest, PeerInfoResponse>>,
    bootstrap_addr: Multiaddr,
    bootstrap_id: &PeerId,
    resolve_id: PeerId,
) -> Result<PeerInfoResponse, anyhow::Error> {
    swarm.dial(bootstrap_addr)?;

    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(event) => match event {
                    request_response::Event::Message { peer, message } => {
                        if let Message::Response {
                            request_id,
                            response,
                        } = message
                        {
                            tracing::info!("Received response with id {request_id} from {peer}");
                            return Ok(response);
                        }
                    }
                    request_response::Event::OutboundFailure {
                        peer,
                        request_id,
                        error,
                    } => {
                        tracing::error!("Failed to send message with id {request_id} to {peer}: {error}");
                        bail!("Failed to send message with id {request_id} to {peer}: {error}")
                    }
                    request_response::Event::InboundFailure {
                        peer,
                        request_id,
                        error,
                    } => {
                        tracing::error!("Failed to receive message with id {request_id} from {peer}: {error}");
                        bail!("Failed to receive message with id {request_id} from {peer}: {error}")
                    }
                    other => tracing::debug!("Unreachable event: {other:?}")
                },
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    tracing::info!("Connected to {}", peer_id);
                    swarm.behaviour_mut().send_request(bootstrap_id, resolve_id.into());
                }
                other => tracing::debug!("Received other event: {other:?}"),
            }
        }
    }
}
