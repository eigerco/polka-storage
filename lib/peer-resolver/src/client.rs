use std::time::Duration;

use anyhow::{bail, Result};
use libp2p::{
    futures::StreamExt,
    identity::Keypair,
    multiaddr::Protocol,
    rendezvous::{client, Cookie, Namespace, Registration},
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
    pub async fn register(
        &mut self,
        rendezvous_point: PeerId,
        rendezvous_point_address: Multiaddr,
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
                            None,
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

    /// This functions connects to the rendezvous_point_address and requests the known peers.
    /// The given rendezvous_point is used to check that the expected PeerId matches the connected one.
    /// Returns registered peers.
    pub async fn initial_discovery(
        &mut self,
        rendezvous_point: PeerId,
        rendezvous_point_address: Multiaddr,
    ) -> Result<Vec<Registration>, ResolverError> {
        self.swarm
            .dial(rendezvous_point_address.clone())
            .map_err(|_| ResolverError::DialError)?;
        loop {
            if let Some(event) = self.swarm.next().await {
                match event {
                    // Connection with rendezvous point established
                    SwarmEvent::ConnectionEstablished { peer_id, .. }
                        if peer_id == rendezvous_point =>
                    {
                        tracing::info!("Connection established with rendezvous point {}", peer_id);
                        tracing::info!("Connected to rendezvous point, discovering nodes");

                        // Requesting rendezvous point for peer discovery
                        self.swarm.behaviour_mut().rendezvous.discover(
                            None,
                            None,
                            None,
                            rendezvous_point,
                        );
                    }
                    SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                        client::Event::Discovered {
                            registrations,
                            cookie: new_cookie,
                            ..
                        },
                    )) => {
                        // set rendezvous cookie
                        self.cookie.replace(new_cookie);

                        for registration in &registrations {
                            for address in registration.record.addresses() {
                                // skip self
                                if &registration.record.peer_id() == self.swarm.local_peer_id() {
                                    continue;
                                }
                                let peer = registration.record.peer_id();
                                tracing::info!(%peer, %address, "Discovered peer");

                                let p2p_suffix = Protocol::P2p(peer);
                                let address_with_p2p = if !address
                                    .ends_with(&Multiaddr::empty().with(p2p_suffix.clone()))
                                {
                                    address.clone().with(p2p_suffix)
                                } else {
                                    address.clone()
                                };

                                self.swarm
                                    .dial(address_with_p2p)
                                    .map_err(|_| ResolverError::DialError)?;
                            }
                        }

                        return Ok(registrations);
                    }
                    _other => {}
                }
            }
        }
    }

    /// This functions continuously listens for new peers at the discover_secs interval.
    /// Only discovered changes in peers since the rendezvous cookie.
    /// The rendezvous_point is used to discover new peers.
    /// This function should be called after the initial_discovery function.
    pub async fn discovery(&mut self, discover_secs: u64, rendezvous_point: PeerId) -> Result<()> {
        if self.cookie.is_none() {
            bail!("Rendezvous cookie should be set");
        }
        let mut discover_tick = tokio::time::interval(Duration::from_secs(discover_secs));
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => match event {
                    // Bootstrap node responded with discovered peer, could be 0 registrations
                    SwarmEvent::Behaviour(ClientBehaviourEvent::Rendezvous(
                        client::Event::Discovered {
                            registrations,
                            cookie: new_cookie,
                            ..
                        },
                    )) => {
                        // set rendezvous cookie
                        self.cookie.replace(new_cookie);

                        for registration in registrations {
                            for address in registration.record.addresses() {
                                // Skip self
                                if &registration.record.peer_id() == self.swarm.local_peer_id() {
                                    continue;
                                }
                                let peer = registration.record.peer_id();
                                tracing::info!(%peer, %address, "Discovered peer");

                                let p2p_suffix = Protocol::P2p(peer);
                                let address_with_p2p = if !address
                                    .ends_with(&Multiaddr::empty().with(p2p_suffix.clone()))
                                {
                                    address.clone().with(p2p_suffix)
                                } else {
                                    address.clone()
                                };

                                self.swarm.dial(address_with_p2p)?;
                            }
                        }
                    }
                    _other => {}
                },
                // Rediscover peers every tick.
                _ = discover_tick.tick() =>
                self.swarm.behaviour_mut().rendezvous.discover(
                    None,
                    self.cookie.clone(),
                    None,
                    rendezvous_point,
                )
            }
        }
    }
}
