use std::time::Duration;

use libp2p::{identity::Keypair, noise, swarm::NetworkBehaviour, tcp, yamux, Swarm, SwarmBuilder};

use super::P2pError;

pub(crate) fn new_swarm<B>(keypair: Keypair, behaviour: B) -> Result<Swarm<B>, P2pError>
where
    B: NetworkBehaviour,
{
    Ok(SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| behaviour)
        .expect("Moving behaviour doesn't fail")
        .with_swarm_config(|config| config.with_idle_connection_timeout(Duration::from_secs(10)))
        .build())
}
