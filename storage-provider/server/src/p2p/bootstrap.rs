use std::time::Duration;

use libp2p::{
    futures::StreamExt, identify, identity::Keypair, noise, rendezvous, swarm::NetworkBehaviour,
    swarm::SwarmEvent, tcp, yamux, Multiaddr, Swarm, SwarmBuilder,
};

use super::P2PError;

#[derive(NetworkBehaviour)]
pub struct BootstrapBehaviour {
    pub rendezvous: rendezvous::server::Behaviour,
    pub identify: identify::Behaviour,
}

pub struct BootstrapConfig {
    address: Multiaddr,
    keypair: Keypair,
}

impl BootstrapConfig {
    pub fn new(keypair: Keypair, address: Multiaddr) -> Self {
        Self { address, keypair }
    }

    pub fn create_swarm(self) -> Result<(Swarm<BootstrapBehaviour>, Multiaddr), P2PError> {
        let swarm = SwarmBuilder::with_existing_identity(self.keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| P2PError::InvalidTcpConfig)?
            .with_behaviour(|key| BootstrapBehaviour {
                // Rendezvous server behaviour for serving new peers to connecting nodes.
                rendezvous: rendezvous::server::Behaviour::new(
                    rendezvous::server::Config::default(),
                ),
                // The identify behaviour is used to share the external address and the public key with connecting clients.
                identify: identify::Behaviour::new(identify::Config::new(
                    "identify/1.0.0".to_string(),
                    key.public(),
                )),
            })
            .map_err(|_| P2PError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        Ok((swarm, self.address))
    }
}

/// Run the rendezvous point (bootstrap node).
/// Listens on the given [`Multiaddr`]
pub(crate) async fn bootstrap(
    mut swarm: Swarm<BootstrapBehaviour>,
    addr: Multiaddr,
) -> Result<(), P2PError> {
    tracing::info!("Starting P2P bootstrap node at {addr}");
    swarm.listen_on(addr)?;
    while let Some(event) = swarm.next().await {
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
                if !registrations.is_empty() {
                    tracing::info!(
                        "Served peer {} with {} new registrations",
                        enquirer,
                        registrations.len()
                    );
                }
            }
            _other => {}
        }
    }
    Ok(())
}
