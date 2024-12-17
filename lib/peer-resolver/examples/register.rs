use anyhow::Result;
use libp2p::{Multiaddr, PeerId};
use peer_resolver::ClientSwarm;
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
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes([1; 32]).unwrap();

    let mut swarm = ClientSwarm::new(keypair, "rendezvous".to_string())?;

    swarm
        .register(rendezvous_point, rendezvous_point_address)
        .await?;
    Ok(())
}
