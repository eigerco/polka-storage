use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use anyhow::Result;
use peer_resolver::{ClientBehaviourEvent, ClientSwarm, Event, Multiaddr, PeerId, SwarmEvent};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let rendezvous_point =
        "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN".parse::<PeerId>()?;
    let rendezvous_point_address = "/ip4/127.0.0.1/tcp/62649".parse::<Multiaddr>()?;
    // Results in peer id 12D3KooWJWoaqZhDaoEFshF7Rh1bpY9ohihFhzcW6d69Lr2NASuq
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes([2; 32]).unwrap();

    let mut swarm = ClientSwarm::new(keypair, "rendezvous".to_string())?;
    let mut discover_tick = tokio::time::interval(Duration::from_secs(2));
    // Use hashmap as a mock database for peer information
    let mut registration_map = HashMap::new();

    swarm.dial(rendezvous_point_address)?;
    loop {
        tokio::select! {
            event = swarm.next() => if let Some(event) = event {
                match event {
                    // Connection with rendezvous point established
                    SwarmEvent::ConnectionEstablished { peer_id, .. }
                        if peer_id == rendezvous_point =>
                    {
                        tracing::info!("Connection established with rendezvous point {}", peer_id);
                        tracing::info!("Connected to rendezvous point, discovering nodes");

                        // Requesting rendezvous point for peer discovery
                        swarm.discover(rendezvous_point);
                    }
                    SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                        Event::Discovered {
                            registrations,
                            cookie: new_cookie,
                            ..
                        },
                    )) => {
                        // set rendezvous cookie
                        swarm.replace_cookie(new_cookie);

                        for registration in &registrations {
                            let peer_id = registration.record.peer_id();
                            // skip self
                            if &peer_id == swarm.local_peer_id() {
                                continue;
                            }
                            let addresses = registration.record.addresses();
                            tracing::info!(%peer_id, "Discovered peer with addresses {addresses:#?}");

                            match registration_map.entry(peer_id) {
                                Entry::Occupied(e) => {
                                    tracing::info!(%peer_id, "Peer updated");
                                    let known_addresses: &mut Vec<Multiaddr> = e.into_mut();
                                    known_addresses.extend(addresses.to_vec());
                                }
                                Entry::Vacant(e) => {
                                    tracing::info!(%peer_id, "New peer entered with addresses {addresses:#?}");
                                    e.insert(addresses.to_vec());
                                }
                            }
                        }
                    }
                    _other => {}
                }
            },
            _ = discover_tick.tick(), if swarm.cookie().is_some() => swarm.discover(rendezvous_point)
        }
    }
}
