use std::time::Duration;

use libp2p::{
    futures::StreamExt, identify, identity::Keypair, noise, rendezvous, rendezvous::Namespace,
    swarm::NetworkBehaviour, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};

use super::P2PError;

#[derive(NetworkBehaviour)]
pub struct RegisterBehaviour {
    pub identify: identify::Behaviour,
    pub rendezvous: rendezvous::client::Behaviour,
}

pub struct RegisterConfig {
    keypair: Keypair,
    rendezvous_point_address: Multiaddr,
    rendezvous_point: PeerId,
}

impl RegisterConfig {
    pub fn new(
        keypair: Keypair,
        rendezvous_point_address: Multiaddr,
        rendezvous_point: PeerId,
    ) -> Self {
        Self {
            keypair,
            rendezvous_point_address,
            rendezvous_point,
        }
    }

    pub fn create_swarm(self) -> Result<(Swarm<RegisterBehaviour>, Multiaddr, PeerId), P2PError> {
        let swarm = SwarmBuilder::with_existing_identity(self.keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| P2PError::InvalidTcpConfig)?
            .with_behaviour(|key| RegisterBehaviour {
                // The identify behaviour is used to receive the bootstrap node's external address and public key.
                identify: identify::Behaviour::new(identify::Config::new(
                    "identify/1.0.0".to_string(),
                    key.public(),
                )),
                // The rendezvous client behaviour allows the bootstrap node to share peer information with us.
                rendezvous: rendezvous::client::Behaviour::new(key.clone()),
            })
            .map_err(|_| P2PError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        Ok((swarm, self.rendezvous_point_address, self.rendezvous_point))
    }
}

/// Register the peer with the rendezvous point.
/// The ttl is how long the peer will remain registered in seconds.
pub(crate) async fn register(
    mut swarm: Swarm<RegisterBehaviour>,
    rendezvous_point: PeerId,
    rendezvous_point_address: Multiaddr,
    ttl: Option<u64>,
    namespace: Namespace,
) -> Result<(), P2PError> {
    tracing::info!("Attempting to register with rendezvous point {rendezvous_point} at {rendezvous_point_address}");
    swarm.dial(rendezvous_point_address.clone())?;

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!("Listening on {}", address);
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                cause: Some(error),
                ..
            } if peer_id == rendezvous_point => {
                tracing::info!("Lost connection to rendezvous point {}", error);
            }
            // once `/identify` did its job, we know our external address and can register
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Identify(
                identify::Event::Received { info, .. },
            )) => {
                // Register our external address.
                tracing::info!("Registering external address {}", info.observed_addr);
                swarm.add_external_address(info.observed_addr);
                if let Err(error) = swarm.behaviour_mut().rendezvous.register(
                    namespace.clone(),
                    rendezvous_point,
                    ttl,
                ) {
                    tracing::error!("Failed to register: {error}");
                    return Err(P2PError::RegistrationFailed(rendezvous_point));
                }
            }
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Rendezvous(
                rendezvous::client::Event::Registered {
                    namespace,
                    ttl,
                    rendezvous_node,
                },
            )) => {
                tracing::info!(
                    "Registered for namespace '{}' at rendezvous point {} for the next {} seconds",
                    namespace,
                    rendezvous_node,
                    ttl
                );
                return Ok(());
            }
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Rendezvous(
                rendezvous::client::Event::RegisterFailed {
                    rendezvous_node,
                    namespace,
                    error,
                },
            )) => {
                tracing::error!(
                    "Failed to register: rendezvous_node={}, namespace={}, error_code={:?}",
                    rendezvous_node,
                    namespace,
                    error
                );
                return Err(P2PError::RegistrationFailed(rendezvous_node));
            }
            _other => {}
        }
    }

    Ok(())
}
