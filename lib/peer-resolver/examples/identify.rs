//! This example registers a peer with the rendezvous point,
//! allowing the peer to be discovered by other peers.
use anyhow::Result;
use peer_resolver::{Multiaddr, PeerId, RegisterSwarm, MAX_TTL};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let rendezvous_point_address = "/ip4/127.0.0.1/tcp/62649".parse::<Multiaddr>()?;
    let rendezvous_point =
        "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN".parse::<PeerId>()?;
    // Results in peer id 12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5
    let keypair_bytes = [1; 32];

    let mut swarm = RegisterSwarm::new(keypair_bytes, "rendezvous".to_string(), 10)?;

    swarm
        .register(rendezvous_point, rendezvous_point_address, Some(MAX_TTL))
        .await?;
    Ok(())
}
