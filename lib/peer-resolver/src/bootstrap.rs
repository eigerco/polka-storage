use std::time::Duration;

use libp2p::{
    futures::StreamExt,
    identify,
    identity::Keypair,
    noise,
    rendezvous::{self, server},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, Swarm, SwarmBuilder,
};

use crate::error::ResolverError;

/// This struct holds all the behaviour for the bootstrap node.
#[derive(NetworkBehaviour)]
pub struct BootstrapBehaviour {
    /// Rendezvous server behaviour for peer discovery
    rendezvous: server::Behaviour,
    identify: identify::Behaviour,
}

/// This struct is used by bootstrap nodes running a swarm aiding in peer discovery
pub struct BootstrapSwarm {
    /// Swarm with [`BootstrapBehaviour`]
    swarm: Swarm<BootstrapBehaviour>,
}

impl BootstrapSwarm {
    /// Create a new [`BootstrapSwarm`] with the given keypair.
    pub fn new<K>(keypair_bytes: K, timeout: u64) -> Result<BootstrapSwarm, ResolverError>
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
            .with_behaviour(|key| BootstrapBehaviour {
                rendezvous: rendezvous::server::Behaviour::new(
                    rendezvous::server::Config::default(),
                ),
                identify: identify::Behaviour::new(identify::Config::new(
                    "rendezvous-example/1.0.0".to_string(),
                    key.public(),
                )),
            })
            .map_err(|_| ResolverError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(timeout)))
            .build();

        Ok(BootstrapSwarm { swarm })
    }

    /// Run the rendezvous point (bootstrap node).
    /// Listens on the given [`Multiaddr`]
    pub async fn run(&mut self, addr: Multiaddr) -> Result<(), ResolverError> {
        self.swarm
            .listen_on(addr)
            .map_err(|_| ResolverError::ListenError)?;
        while let Some(event) = self.swarm.next().await {
            match event {
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    tracing::info!("Connected to {}", peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    tracing::info!("Disconnected from {}", peer_id);
                }
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::Rendezvous(
                    rendezvous::server::Event::PeerRegistered { peer, registration },
                )) => {
                    tracing::info!(
                        "Peer {} registered for namespace '{}' for {} seconds",
                        peer,
                        registration.namespace,
                        registration.ttl
                    );
                }
                SwarmEvent::Behaviour(BootstrapBehaviourEvent::Rendezvous(
                    rendezvous::server::Event::DiscoverServed {
                        enquirer,
                        registrations,
                    },
                )) => {
                    if registrations.len() > 0 {
                        tracing::info!(
                            "Served peer {} with {} new registrations",
                            enquirer,
                            registrations.len()
                        );
                    }
                }
                other => {
                    tracing::debug!("Unhandled event: {other:?}");
                }
            }
        }
        Ok(())
    }
}
