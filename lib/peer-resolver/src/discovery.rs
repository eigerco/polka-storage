use std::time::Duration;

use libp2p::{
    futures::StreamExt,
    identity::Keypair,
    noise,
    rendezvous::{client, Cookie},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};

use crate::error::ResolverError;

/// This struct holds the rendezvous behaviour for discovery.
#[derive(NetworkBehaviour)]
pub struct DiscoveryBehaviour {
    rendezvous: client::Behaviour,
}

/// Rendezvous discover swarm
pub struct DiscoverySwarm {
    /// Swarm with [`DiscoveryBehaviour`]
    swarm: Swarm<DiscoveryBehaviour>,
    /// Rendezvous cookie for continuous peer discovery
    cookie: Option<Cookie>,
}

impl DiscoverySwarm {
    /// Create a new [`DiscoverySwarm`] with the given keypair.
    /// The given timeout is set for the idle connection timeout
    pub fn new<K>(keypair_bytes: K, timeout: Duration) -> Result<DiscoverySwarm, ResolverError>
    where
        K: AsMut<[u8]>,
    {
        let keypair = Keypair::ed25519_from_bytes(keypair_bytes)?;
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| ResolverError::InvalidTCPConfig)?
            .with_behaviour(|key| DiscoveryBehaviour {
                rendezvous: client::Behaviour::new(key.clone()),
            })
            .map_err(|_| ResolverError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(timeout))
            .build();

        Ok(DiscoverySwarm {
            swarm,
            cookie: None,
        })
    }

    /// This functions requests the rendezvous_point for new or updated peers since the last cookie.
    pub fn discover(&mut self, rendezvous_point: PeerId) {
        self.swarm.behaviour_mut().rendezvous.discover(
            None,
            self.cookie.clone(),
            None,
            rendezvous_point,
        )
    }

    /// Get next swarm event
    pub async fn next(&mut self) -> Option<SwarmEvent<DiscoveryBehaviourEvent>> {
        self.swarm.next().await
    }

    /// Dial the given address
    pub fn dial(&mut self, addr: Multiaddr) -> Result<(), ResolverError> {
        self.swarm.dial(addr).map_err(|_| ResolverError::DialError)
    }

    /// Replace the rendezvous cookie
    pub fn replace_cookie(&mut self, new_cookie: Cookie) {
        self.cookie.replace(new_cookie);
    }

    /// Get the swarms local peer id
    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    /// Get the rendezvous cookie
    pub fn cookie(&self) -> &Option<Cookie> {
        &self.cookie
    }
}
