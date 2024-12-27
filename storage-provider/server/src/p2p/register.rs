use std::{path::PathBuf, str::FromStr, time::Duration};

use libp2p::{
    identify, noise, rendezvous, swarm::NetworkBehaviour, tcp, yamux, Multiaddr, PeerId, Swarm,
    SwarmBuilder,
};
use serde::{de, Deserialize};

use super::{create_keypair, P2PError};
#[derive(NetworkBehaviour)]
pub struct RegisterBehaviour {
    pub identify: identify::Behaviour,
    pub rendezvous: rendezvous::client::Behaviour,
}

fn string_to_peer_id<'de, D: de::Deserializer<'de>>(d: D) -> Result<PeerId, D::Error> {
    let s: String = de::Deserialize::deserialize(d)?;
    PeerId::from_str(&s).map_err(de::Error::custom)
}

#[derive(Deserialize)]
pub struct RegisterConfig {
    key_path: PathBuf,
    rendezvous_point_address: Multiaddr,
    #[serde(deserialize_with = "string_to_peer_id")]
    rendezvous_point: PeerId,
}

impl RegisterConfig {
    pub fn create_swarm(self) -> Result<(Swarm<RegisterBehaviour>, Multiaddr, PeerId), P2PError> {
        let keypair = create_keypair(&self.key_path)?;
        let swarm = SwarmBuilder::with_existing_identity(keypair)
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
