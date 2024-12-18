use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::rendezvous::Namespace;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, noise, rendezvous, tcp, yamux, Multiaddr, PeerId, Swarm};

use crate::error::ResolverError;

#[derive(NetworkBehaviour)]
pub struct RegisterBehaviour {
    identify: identify::Behaviour,
    rendezvous: rendezvous::client::Behaviour,
}

pub struct RegisterSwarm {
    swarm: Swarm<RegisterBehaviour>,
    namespace: Namespace,
}

impl RegisterSwarm {
    /// Create a new [`RegisterSwarm`] with the given keypair.
    /// The namespace is stored for later use.
    /// The given timeout is set for the idle connection timeout
    pub fn new(keypair: Keypair, namespace: String, timeout: u64) -> Result<RegisterSwarm, ResolverError> {
        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| ResolverError::InvalidTCPConfig)?
            .with_behaviour(|key| RegisterBehaviour {
                identify: identify::Behaviour::new(identify::Config::new(
                    "rendezvous-example/1.0.0".to_string(),
                    key.public(),
                )),
                rendezvous: rendezvous::client::Behaviour::new(key.clone()),
            })
            .map_err(|_| ResolverError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(timeout)))
            .build();
        Ok(RegisterSwarm {
            swarm,
            namespace: Namespace::new(namespace)?,
        })
    }

    pub async fn register(
        &mut self,
        rendezvous_point: PeerId,
        rendezvous_point_address: Multiaddr,
        ttl: Option<u64>,
    ) -> Result<(), ResolverError> {
        self.swarm
            .dial(rendezvous_point_address.clone())
            .map_err(|_| ResolverError::DialError)?;

        while let Some(event) = self.swarm.next().await {
            match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    tracing::info!("Listening on {}", address);
                }
                SwarmEvent::ConnectionClosed {
                    peer_id,
                    cause: Some(error),
                    ..
                } if peer_id == rendezvous_point => {
                    tracing::error!("Lost connection to rendezvous point {}", error);
                }
                // once `/identify` did its job, we know our external address and can register
                SwarmEvent::Behaviour(RegisterBehaviourEvent::Identify(
                    identify::Event::Received { info, .. },
                )) => {
                    // Register our external address.
                    tracing::info!("Registering external address {}", info.observed_addr);
                    self.swarm.add_external_address(info.observed_addr);
                    if let Err(error) = self.swarm.behaviour_mut().rendezvous.register(
                        self.namespace.clone(),
                        rendezvous_point,
                        ttl,
                    ) {
                        tracing::error!("Failed to register: {error}");
                        return Err(ResolverError::DialError);
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
                    return Err(ResolverError::RegistrationFailed(error));
                }
                _other => {}
            }
        }

        Ok(())
    }
}
