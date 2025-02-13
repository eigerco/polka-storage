//! This example show how to connect to a bootstrap node within the P2P network
//! and request a Peer ID to Multiaddrs mapping.
//! This client uses libp2p's request response protocol to request a Multiaddrs
//! connected to a given Peer ID.
//! The Multiaddr of the bootstrap node needs to be known because the client
//! needs to dial (connect) to the bootstrap node to send a request.
//! The Peer ID of the bootstrap node needs to be known to send the request
//! to the bootstrap node.
use std::time::Duration;

use anyhow::{bail, Result};
use clap::Parser;
use libp2p::futures::StreamExt;
use libp2p::request_response::{Message, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{
    noise, request_response, tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use primitives::p2p::{PeerInfo, WPeerId};
use tracing_subscriber::EnvFilter;

/// Create a discovery swarm
fn create_discover_swarm() -> Result<Swarm<request_response::cbor::Behaviour<WPeerId, PeerInfo>>> {
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
                    StreamProtocol::new("/resolver/1.0.0"),
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
    mut swarm: Swarm<request_response::cbor::Behaviour<WPeerId, PeerInfo>>,
    bootstrap_addr: Multiaddr,
    bootstrap_id: &PeerId,
    resolve_id: PeerId,
) -> Result<PeerInfo> {
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
                        bail!("Failed to send message with id {request_id} to {peer}: {error}");
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

#[derive(Parser)]
struct Cli {
    /// Multiaddr of the bootstrap node.
    #[arg(long)]
    bootstrap_addr: Multiaddr,
    /// PeerID of the bootstrap node.
    #[arg(long)]
    bootstrap_id: PeerId,
    /// Peer ID to request the Multiaddrs for.
    #[arg(long)]
    resolve_id: PeerId,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let cli = Cli::parse();
    let swarm = create_discover_swarm()?;
    println!("Attempting to get multiaddrs for peer {:?}", cli.resolve_id);
    let peer_info =
        run_discover(swarm, cli.bootstrap_addr, &cli.bootstrap_id, cli.resolve_id).await?;
    println!(
        "Got multiaddrs {:#?} for peer {:?}",
        peer_info.multiaddrs, peer_info.peer_id
    );
    Ok(())
}
