use anyhow::Result;
use peer_resolver::BootstrapSwarm;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    // Results in PeerID 12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN which is
    // used as the rendezvous point by the other peer examples.
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes([0; 32]).unwrap();

    let mut swarm = BootstrapSwarm::new(keypair, 10)?;

    swarm.run("/ip4/0.0.0.0/tcp/62649".parse()?).await?;

    Ok(())
}
