//! This example starts a bootstrap node (rendezvous point).
//! It listened for incoming connections and handles peer registration and discovery.
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
    let keypair_bytes = [0; 32];

    let mut swarm = BootstrapSwarm::new(keypair_bytes, 10)?;

    swarm.run("/ip4/0.0.0.0/tcp/62649".parse()?).await?;

    Ok(())
}
