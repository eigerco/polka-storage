use std::time::Duration;

use libp2p::{
    identify, identity::Keypair, noise, rendezvous, swarm::NetworkBehaviour, tcp, yamux, Multiaddr,
    PeerId, Swarm, SwarmBuilder,
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
                identify: identify::Behaviour::new(identify::Config::new(
                    "identify/1.0.0".to_string(),
                    key.public(),
                )),
                rendezvous: rendezvous::client::Behaviour::new(key.clone()),
            })
            .map_err(|_| P2PError::InvalidBehaviourConfig)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        Ok((swarm, self.rendezvous_point_address, self.rendezvous_point))
    }
}
