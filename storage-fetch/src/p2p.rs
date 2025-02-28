use std::{sync::Arc, time::Duration};

use ::blockstore::Blockstore;
use libp2p::{noise, swarm::NetworkBehaviour, tcp, yamux, Swarm, SwarmBuilder};
use thiserror::Error;

const MAX_MULTIHASH_LENGTH: usize = 64;

/// Custom Behaviour used by the server and client.
#[derive(NetworkBehaviour)]
pub struct Behaviour<B>
where
    B: Blockstore + 'static,
{
    pub bitswap: beetswap::Behaviour<MAX_MULTIHASH_LENGTH, B>,
}

/// Error that can occur while initializing a swarm
#[derive(Debug, Error)]
pub enum InitSwarmError {
    /// Failed to initialize noise protocol.
    #[error("Failed to initialize noise: {0}")]
    Noise(#[from] noise::Error),
}

/// Initialize a new swarm with our custom Behaviour.
pub fn new_swarm<B>(blockstore: Arc<B>) -> Result<Swarm<Behaviour<B>>, InitSwarmError>
where
    B: Blockstore + 'static,
{
    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| Behaviour {
            bitswap: beetswap::Behaviour::new(blockstore),
        })
        .expect("Moving behaviour doesn't fail")
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}
