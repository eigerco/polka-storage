//! This example attempts to discovery peers registered with the rendezvous point.
//! It continuously, with an interval of 2 seconds, requests any newly registered
//! peers information from the rendezvous point.
//! It will insert or update a hashmap with the peer information as new peer info is discovered.
use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use anyhow::Result;
use peer_resolver::{
    DiscoveryBehaviourEvent, DiscoverySwarm, Event, Multiaddr, PeerId, SwarmEvent,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    // Rendezvous peer id and multiaddr for the bootstrap example.
    let rendezvous_point =
        "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN".parse::<PeerId>()?;
    let rendezvous_point_address = "/ip4/127.0.0.1/tcp/62649".parse::<Multiaddr>()?;
    // Results in peer id 12D3KooWJWoaqZhDaoEFshF7Rh1bpY9ohihFhzcW6d69Lr2NASuq
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes([2; 32]).unwrap();
    // Create a new discovery swarm with the above keypair and a timeout of 10 seconds
    let mut swarm = DiscoverySwarm::new(keypair, 10)?;
    // Set discovery tick for discover request at 2 seconds
    let mut discover_tick = tokio::time::interval(Duration::from_secs(2));
    // Use hashmap as a mock database for peer information
    let mut registration_map = HashMap::new();

    // Dial in to the rendezvous point.
    swarm.dial(rendezvous_point_address)?;
    loop {
        tokio::select! {
            // Check incoming event for the discovery swarm.
            event = swarm.next() => if let Some(event) = event {
                match event {
                    // Connection with rendezvous point established
                    SwarmEvent::ConnectionEstablished { peer_id, .. }
                        if peer_id == rendezvous_point =>
                    {
                        tracing::info!("Connection established with rendezvous point {}", peer_id);
                        tracing::info!("Connected to rendezvous point, discovering nodes...");

                        // Requesting rendezvous point for peer discovery
                        swarm.discover(rendezvous_point);
                    }
                    // Received discovered event from the rendezvous point
                    SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Rendezvous(
                        Event::Discovered {
                            registrations,
                            cookie: new_cookie,
                            ..
                        },
                    )) => {
                        // set rendezvous cookie
                        swarm.replace_cookie(new_cookie);

                        // Check registrations
                        for registration in &registrations {
                            // Get peer ID from the registration record
                            let peer_id = registration.record.peer_id();
                            // skip self
                            if &peer_id == swarm.local_peer_id() {
                                continue;
                            }
                            let addresses = registration.record.addresses();

                            // Enter new registration in the 'db' and update existing if anything changed.
                            match registration_map.entry(peer_id) {
                                Entry::Occupied(e) => {
                                    // Peer is already registered.
                                    // Check if the address is already contained in the map.
                                    // Update the address if needed.
                                    let known_addresses: &mut Vec<Multiaddr> = e.into_mut();
                                    for address in addresses {
                                        if !known_addresses.contains(address) {
                                            known_addresses.push(address.clone());
                                            tracing::info!(%peer_id, %address, "Peer updated");
                                        }
                                    }
                                }
                                Entry::Vacant(e) => {
                                    // Add the new peer to the mapping.
                                    tracing::info!(%peer_id, "New peer entered with addresses {addresses:#?}");
                                    e.insert(addresses.to_vec());
                                }
                            }
                        }
                    }
                    other => { tracing::debug!("Unhandled event: {other:?}"); }
                }
            },
            // Re-request discovery from the rendezvous point after 2 second interval
            _ = discover_tick.tick(), if swarm.cookie().is_some() => swarm.discover(rendezvous_point)
        }
    }
}
