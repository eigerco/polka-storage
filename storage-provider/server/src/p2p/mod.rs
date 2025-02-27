use std::{str::FromStr, sync::Arc, time::Duration};

use ::blockstore::Blockstore;
use futures::StreamExt;
use libp2p::{
    identify::{self, Event as IdentifyEvent},
    identity::Keypair,
    rendezvous::{self, client::Event as RendezvousEvent, Namespace},
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, Swarm,
};
use primitives::p2p::{keypair_value_parser, DEFAULT_REGISTRATION_TTL};
use serde::de;
use swarm::new_swarm;
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};

pub mod blockstore;
mod error;
mod swarm;

pub use error::P2pError;

/// The time-to-live duration for node registration with rendezvous points.
const REGISTRATION_TTL: Duration = Duration::from_secs(DEFAULT_REGISTRATION_TTL);

/// Maximum length allowed for a multihash in bytes.
const MAX_MULTIHASH_LENGTH: usize = 64;

/// Unique namespace used for peer discovery and registration with rendezvous nodes.
const P2P_NAMESPACE: &str = "polka-storage";

/// The protocol version identifier string used by the identify protocol.
const IDENTIFY_PROTOCOL_VERSION: &str = "polka-storage/1.0.0";

/// Starts a new P2P networking service in a separate tokio task.
pub fn start_p2p<B>(
    args: P2pArgs<B>,
    cancellation_token: CancellationToken,
) -> Result<JoinHandle<Result<(), P2pError>>, P2pError>
where
    B: Blockstore + Send + 'static,
{
    // Initialize the p2p worker and move it to the different task
    let worker = Worker::new(args)?;

    Ok(tokio::spawn(async move {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                tracing::info!("P2P worker received shutdown signal");
            }
            _ = worker.run() => {
                tracing::info!("P2P worker completed");
            }
        }

        Ok(())
    }))
}

/// Arguments used to configure the [`P2p`].
pub struct P2pArgs<B>
where
    B: Blockstore,
{
    /// The keypair to be used as the identity.
    pub local_keypair: Keypair,
    /// List of rendezvous nodes to register to.
    pub rendezvous_nodes: Vec<(PeerId, Multiaddr)>,
    /// List of the addresses on which to listen for incoming connections.
    pub listen_on: Vec<Multiaddr>,
    /// The blockstore used for content retrieval.
    pub blockstore: Arc<B>,
}

/// Our network behaviour.
#[derive(NetworkBehaviour)]
pub struct Behaviour<B>
where
    B: Blockstore + 'static,
{
    identify: identify::Behaviour,
    rendezvous: rendezvous::client::Behaviour,
    bitswap: beetswap::Behaviour<MAX_MULTIHASH_LENGTH, B>,
}

struct Worker<B>
where
    B: Blockstore + 'static,
{
    swarm: Swarm<Behaviour<B>>,
    rendezvous_nodes: Vec<(PeerId, Multiaddr)>,
}

impl<B> Worker<B>
where
    B: Blockstore,
{
    pub fn new(args: P2pArgs<B>) -> Result<Self, P2pError> {
        let identify = identify::Behaviour::new(identify::Config::new(
            IDENTIFY_PROTOCOL_VERSION.to_string(),
            args.local_keypair.public(),
        ));

        let rendezvous = rendezvous::client::Behaviour::new(args.local_keypair.clone());

        let bitswap = beetswap::Behaviour::new(args.blockstore);

        let behaviour = Behaviour {
            identify,
            rendezvous,
            bitswap,
        };

        let mut swarm = new_swarm(args.local_keypair, behaviour)?;

        for addr in args.listen_on {
            swarm.listen_on(addr)?;
        }

        // We are dialing the rendezvous nodes. After the connection is
        // successfully established, the identify message received from the
        // nodes tells us our public multiaddr which we'll register.
        let rendezvous_nodes = dial_rendezvous_nodes(&mut swarm, &args.rendezvous_nodes)?;

        Ok(Worker {
            swarm,
            rendezvous_nodes,
        })
    }

    async fn run(mut self) -> Result<(), P2pError> {
        let mut register_interval = tokio::time::interval(REGISTRATION_TTL);

        loop {
            select! {
                _ = register_interval.tick() => {
                    // Check if there are any external addresses set, before we
                    // actually try to register ourself with the rendezvous nodes
                    if self.swarm.external_addresses().count() == 0 {
                        debug!("External address not known. Skip registration.");
                        register_interval.reset_after(Duration::from_secs(1));
                        continue;
                    }

                    // Dial rendezvous nodes again because the connection is not persisted
                    dial_rendezvous_nodes(&mut self.swarm, &self.rendezvous_nodes)?;

                    // Register with the nodes
                    request_registration(&mut self.swarm, &self.rendezvous_nodes);
                }
                event = self.swarm.select_next_some() => self.on_swarm_event(event),
            }
        }
    }

    fn on_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent<B>>) {
        match event {
            SwarmEvent::Behaviour(ev) => match ev {
                BehaviourEvent::Identify(ev) => self.on_identify_event(ev),
                BehaviourEvent::Rendezvous(ev) => self.on_rendezvous_event(ev),
                _ => {}
            },
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(%address, "Listening on");
            }
            _ => {}
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn on_identify_event(&mut self, event: IdentifyEvent) {
        match event {
            // once `/identify` did its job, we know our external address
            IdentifyEvent::Received { info, .. } => {
                self.swarm.add_external_address(info.observed_addr);
            }
            _ => {}
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn on_rendezvous_event(&mut self, event: RendezvousEvent) {
        match event {
            RendezvousEvent::Registered {
                rendezvous_node,
                ttl,
                ..
            } => {
                info!(%rendezvous_node, %ttl, "Successfully registered");
            }
            RendezvousEvent::RegisterFailed {
                rendezvous_node,
                error,
                ..
            } => {
                warn!(%rendezvous_node, ?error, "Registration failed");
            }
            RendezvousEvent::Expired { peer } => {
                debug!(%peer, "Registration expired");
            }
            _ => {}
        }
    }
}

/// Dials rendezvous nodes. The `Ok` indicates that we successfully started a
/// dialing procedure with the nodes. It doesn't indicate that we successfully
/// connected to the nodes.
fn dial_rendezvous_nodes<B>(
    swarm: &mut Swarm<Behaviour<B>>,
    nodes: &[(PeerId, Multiaddr)],
) -> Result<Vec<(PeerId, Multiaddr)>, P2pError>
where
    B: Blockstore,
{
    let mut dialling = vec![];
    for (rendezvous_peer, rendezvous_addr) in nodes {
        // Start dialing the node if needed
        if !swarm.is_connected(rendezvous_peer) {
            if let Err(err) = swarm.dial(rendezvous_addr.clone()) {
                warn!(?err, %rendezvous_peer, %rendezvous_addr, "rendezvous node dialing error");
                continue;
            }
        }

        dialling.push((*rendezvous_peer, rendezvous_addr.clone()));
    }

    // Return error if we cant dial any nodes
    if dialling.is_empty() {
        Err(P2pError::NoRendezvousNodesAvailable)
    } else {
        Ok(dialling)
    }
}

/// Request registration with the rendezvous nodes.
fn request_registration<B>(swarm: &mut Swarm<Behaviour<B>>, nodes: &[(PeerId, Multiaddr)])
where
    B: Blockstore,
{
    for (rendezvous_peer, rendezvous_addr) in nodes {
        // Register with the node
        let request_result = swarm.behaviour_mut().rendezvous.register(
            Namespace::from_static(P2P_NAMESPACE),
            *rendezvous_peer,
            Some(REGISTRATION_TTL.as_secs()),
        );

        match request_result {
            Ok(_) => {
                info!(%rendezvous_peer, %rendezvous_addr, "Registration requested");
            }
            Err(err) => {
                warn!(?err, %rendezvous_peer, %rendezvous_addr, "Registration request failed");
            }
        }
    }
}

/// Deserializes a ED25519 private key into a Keypair.
/// Can either be the private key as a string or the path of a PEM file with an @ prefixed
/// Calls `keypair_value_parser` after deserializing the source string
pub(crate) fn deser_keypair<'de, D: de::Deserializer<'de>>(d: D) -> Result<Keypair, D::Error> {
    let src: String = de::Deserialize::deserialize(d)?;
    keypair_value_parser(&src).map_err(de::Error::custom)
}

/// Parses a string to an Peer ID.
pub(crate) fn deserialize_string_to_peer_id<'de, D: de::Deserializer<'de>>(
    d: D,
) -> Result<PeerId, D::Error> {
    let s: String = de::Deserialize::deserialize(d)?;
    PeerId::from_str(&s).map_err(de::Error::custom)
}
