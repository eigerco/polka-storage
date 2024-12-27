use std::{path::PathBuf, time::Duration};

use libp2p::{
    identify, noise, rendezvous, swarm::NetworkBehaviour, tcp, yamux, Multiaddr, Swarm,
    SwarmBuilder,
};
use serde::Deserialize;

use super::{create_keypair, P2PError};

#[derive(NetworkBehaviour)]
pub struct BootstrapBehaviour {
    pub rendezvous: rendezvous::server::Behaviour,
    pub identify: identify::Behaviour,
}

#[derive(Deserialize)]
pub struct BootstrapConfig {
    address: Multiaddr,
    key_path: PathBuf,
}

impl BootstrapConfig {
    pub fn create_swarm(self) -> Result<(Swarm<BootstrapBehaviour>, Multiaddr), P2PError> {
        let keypair = create_keypair(&self.key_path)?;
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|_| P2PError::InvalidTcpConfig)?
            .with_behaviour(|key| BootstrapBehaviour {
                rendezvous: rendezvous::server::Behaviour::new(
                    rendezvous::server::Config::default(),
                ),
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
