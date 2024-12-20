mod bootstrap;
mod discovery;
mod error;
mod register;

pub use bootstrap::{BootstrapBehaviour, BootstrapBehaviourEvent, BootstrapSwarm};
pub use discovery::{DiscoveryBehaviour, DiscoveryBehaviourEvent, DiscoverySwarm};
pub use error::ResolverError;
pub use libp2p::{
    rendezvous::{client::Event, MAX_TTL, MIN_TTL},
    swarm::SwarmEvent,
    Multiaddr, PeerId,
};
pub use register::{RegisterBehaviour, RegisterBehaviourEvent, RegisterSwarm};
