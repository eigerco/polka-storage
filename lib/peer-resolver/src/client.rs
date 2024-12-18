use anyhow::Result;
use libp2p::{
    futures::StreamExt,
    identity::Keypair,
    rendezvous::{client, Cookie, Namespace},
    swarm::{NetworkBehaviour, SwarmEvent},
    {noise, tcp, yamux, Multiaddr, PeerId, Swarm},
};

use crate::error::ResolverError;

/// This struct holds the rendezvous client behaviour for registration and discovery.
#[derive(NetworkBehaviour)]
pub struct ClientBehaviour {
    rendezvous: client::Behaviour,
}

/// Rendezvous client swarm
pub struct ClientSwarm {
    /// Swarm with [`ClientBehaviour`]
    swarm: Swarm<ClientBehaviour>,
    /// The namespace the client joins
    namespace: Namespace,
    /// Rendezvous cookie for continuous peer discovery
    cookie: Option<Cookie>,
}

impl ClientSwarm {
    /// Create a new [`ClientSwarm`] with the given keypair.
    /// The namespace is stored for later use.
    pub fn new(keypair: Keypair, namespace: String) -> Result<ClientSwarm, ResolverError> {
        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| ResolverError::InvalidTCPConfig)?
            .with_behaviour(|key| ClientBehaviour {
                rendezvous: client::Behaviour::new(key.clone()),
            })
            .map_err(|_| ResolverError::InvalidBehaviourConfig)?
            .build();

        Ok(ClientSwarm {
            swarm,
            namespace: Namespace::new(namespace)?,
            cookie: None,
        })
    }

    /// Register the peer to the namespace given in the new function.
    /// Adds the given rendezvous_point_address to external addresses and dials it to register.
    /// The given rendezvous_point is used to check that the expected PeerId matches the connected one.
    /// The ttl argument sets how long the peer is registered in seconds, defaults to 2 hours (min), max is 72 hours
    pub async fn register(
        &mut self,
        rendezvous_point: PeerId,
        rendezvous_point_address: Multiaddr,
        ttl: Option<u64>,
    ) -> Result<(), ResolverError> {
        self.swarm
            .add_external_address(rendezvous_point_address.clone());
        self.swarm
            .dial(rendezvous_point_address)
            .map_err(|_| ResolverError::DialError)?;
        loop {
            if let Some(event) = self.swarm.next().await {
                match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. }
                        if peer_id == rendezvous_point =>
                    {
                        if let Err(error) = self.swarm.behaviour_mut().rendezvous.register(
                            self.namespace.clone(),
                            rendezvous_point,
                            ttl,
                        ) {
                            tracing::error!("Failed to register: {error}");
                            return Err(ResolverError::RegisterError(error));
                        }
                        tracing::info!("Connection established with rendezvous point {}", peer_id);
                    }
                    SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                        client::Event::Registered {
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
                    SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                        client::Event::RegisterFailed {
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
        }
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
    pub async fn next(&mut self) -> Option<SwarmEvent<ClientBehaviourEvent>> {
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
