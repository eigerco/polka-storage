use libp2p::{
    futures::StreamExt,
    identify,
    identity::Keypair,
    noise,
    rendezvous::{self, Namespace},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::time::Duration;

use super::P2PError;
use crate::p2p::TTL_24_HOURS;

#[derive(NetworkBehaviour)]
pub struct RegisterBehaviour {
    pub identify: identify::Behaviour,
    pub rendezvous: rendezvous::client::Behaviour,
}

pub struct RegisterConfig {
    keypair: Keypair,
    pub(crate) rendezvous_point_address: Multiaddr,
    pub(crate) rendezvous_point: PeerId,
    pub(crate) registration_ttl: u64,
}

impl RegisterConfig {
    pub fn new(
        keypair: Keypair,
        rendezvous_point_address: Multiaddr,
        rendezvous_point: PeerId,
        registration_ttl: u64,
    ) -> Self {
        Self {
            keypair,
            rendezvous_point_address,
            rendezvous_point,
            registration_ttl,
        }
    }

    pub fn create_swarm(self) -> Result<Swarm<RegisterBehaviour>, P2PError> {
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

        Ok(swarm)
    }
}

/// Register the peer with the rendezvous point.
/// The ttl is how long the peer will remain registered in seconds.
#[tracing::instrument(
    skip(swarm),
    fields(
        rendezvous_point = %rendezvous_point,
        rendezvous_point_address = %rendezvous_point_address,
        ttl = %ttl,
        namespace = %namespace
    )
)]
pub(crate) async fn register(
    swarm: &mut Swarm<RegisterBehaviour>,
    rendezvous_point: PeerId,
    rendezvous_point_address: Multiaddr,
    ttl: u64,
    namespace: Namespace,
) -> Result<(), P2PError> {
    tracing::info!("Attempting to register with rendezvous point {rendezvous_point} at {rendezvous_point_address}");
    let mut register_tick = tokio::time::interval(Duration::from_secs(ttl));

    // Dial into bootstrap address
    swarm.dial(rendezvous_point_address.clone())?;
    // Get and add external address
    let external_addr = get_external_address(swarm).await;
    swarm.add_external_address(external_addr);

    loop {
        tokio::select! {
            // Poll tick every TTL to re-register.
            // First tick completes immediately.
            _ = register_tick.tick() => {
                tracing::info!("Registering with p2p node");
                // Dial to establish a connection.
                // Dial is needed because the connection is not kept alive using rendezvous protocol.
                swarm.dial(rendezvous_point_address.clone())?;
                // Register with bootstrap node.
                if let Err(error) =
                    swarm
                        .behaviour_mut()
                        .rendezvous
                        .register(namespace.clone(), rendezvous_point, Some(ttl))
                {
                    tracing::error!("Failed to register: {error}");
                    return Err(P2PError::RegistrationFailed(rendezvous_point));
                } else {
                    tracing::info!("Registration requested with {rendezvous_point}");
                }
            }
            // Check incoming event.
            event = swarm.select_next_some() => on_swarm_event(event)?
        }
    }
}

/// Checks swarm events related to registration and returns an error if the registration failed.
fn on_swarm_event(event: SwarmEvent<RegisterBehaviourEvent>) -> Result<(), P2PError> {
    match event {
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
        }
        SwarmEvent::Behaviour(RegisterBehaviourEvent::Rendezvous(
            rendezvous::client::Event::RegisterFailed {
                rendezvous_node,
                namespace,
                error,
            },
        )) => {
            tracing::error!(%rendezvous_node, %namespace,
                "Failed to register error = {error:?}"
            );
            return Err(P2PError::RegistrationFailed(rendezvous_node));
        }
        other => tracing::debug!("Encountered event: {other:?}"),
    }
    Ok(())
}

/// Checks the swarm for the `Identify` event to get its external address
/// so we can add it to the swarm with `add_external_address`.
async fn get_external_address(swarm: &mut Swarm<RegisterBehaviour>) -> Multiaddr {
    loop {
        match swarm.select_next_some().await {
            // once `/identify` did its job, we know our external address and can return it
            SwarmEvent::Behaviour(RegisterBehaviourEvent::Identify(
                identify::Event::Received { info, .. },
            )) => {
                tracing::info!("Identity information exchanged, external address received");
                return info.observed_addr;
            }
            other => tracing::debug!("Encountered event: {other:?}"),
        }
    }
}
