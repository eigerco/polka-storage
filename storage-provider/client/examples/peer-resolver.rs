//! Peer Resolver example
//!
//! This example shows how to use the rendezvous client protocol to
//! connect to rendezvous bootstrap, and send a discovery message,
//! requesting the bootstrap node to return their registrations.
//! Then it will check the registrations to see if a given Peer ID
//! is contained in them to get a Peer ID to multiaddr mapping.
//! If the bootstrap node does not have information on the given Peer
//! ID, the example will return an error.
//! NOTE: This example is to be removed and implemented into the
//! client at some point.
use std::{error::Error, time::Duration};

use clap::Parser;
use libp2p::{
    futures::StreamExt,
    noise,
    rendezvous::client::{Behaviour, Event},
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tracing_subscriber::EnvFilter;

#[derive(Debug)]
struct PeerInfo {
    peer_id: PeerId,
    multiaddresses: Vec<Multiaddr>,
}

#[derive(Parser)]
struct Cli {
    /// Peer ID to resolve
    #[arg(long)]
    peer_id: PeerId,

    /// Rendezvous point address of the bootstrap node
    #[arg(long)]
    rendezvous_point_address: Multiaddr,

    /// PeerID of the bootstrap node
    #[arg(long)]
    rendezvous_point: PeerId,
}

fn create_swarm() -> Result<Swarm<Behaviour>, Box<dyn Error>> {
    Ok(SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| Behaviour::new(key.clone()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
        .build())
}

async fn discover(
    swarm: &mut Swarm<Behaviour>,
    peer_id_to_find: PeerId,
    rendezvous_point_address: Multiaddr,
    rendezvous_point: PeerId,
) -> Result<PeerInfo, Box<dyn Error>> {
    // Dial in to the rendezvous point.
    swarm.dial(rendezvous_point_address)?;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                if peer_id == rendezvous_point {
                    tracing::info!("Connection established with rendezvous point {}", peer_id);

                    // Requesting rendezvous point for peer discovery
                    swarm
                        .behaviour_mut()
                        .discover(None, None, None, rendezvous_point);
                }
            }
            // Received discovered event from the rendezvous point
            SwarmEvent::Behaviour(Event::Discovered { registrations, .. }) => {
                // Check registrations
                for registration in &registrations {
                    // Get peer ID from the registration record
                    let peer_id = registration.record.peer_id();
                    // skip self
                    if &peer_id == swarm.local_peer_id() {
                        continue;
                    }
                    if peer_id == peer_id_to_find {
                        return Ok(PeerInfo {
                            peer_id,
                            multiaddresses: registration.record.addresses().to_vec(),
                        });
                    }
                }
                return Err(format!(
                    "No registered multi-addresses found for Peer ID {peer_id_to_find}"
                )
                .into());
            }

            other => tracing::debug!("Other event: {other:?}"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let args = Cli::parse();

    let mut swarm = create_swarm()?;
    match discover(
        &mut swarm,
        args.peer_id,
        args.rendezvous_point_address,
        args.rendezvous_point,
    )
    .await
    {
        Ok(peer_info) => {
            println!("Found peer with Peer ID {}", args.peer_id);
            println!("Peer Info:");
            println!("Peer ID: {}", peer_info.peer_id);
            println!("Multiaddresses: {:?}", peer_info.multiaddresses);
        }
        Err(e) => eprintln!("{e}"),
    }

    Ok(())
}
