use std::time::Duration;

use libp2p::{
    identify,
    identity::Keypair,
    kad, noise,
    request_response::{self, ProtocolSupport},
    tcp, yamux, StreamProtocol, Swarm, SwarmBuilder,
};
use primitives_p2p::{BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL, IDENTIFY_PROTOCOL_VERSION};

use crate::{bootstrap::BootstrapBehaviour, P2PError};

pub async fn create_swarm(keypair: Option<Keypair>) -> Result<Swarm<BootstrapBehaviour>, P2PError> {
    Ok(keypair
        .map_or_else(
            SwarmBuilder::with_new_identity,
            SwarmBuilder::with_existing_identity,
        )
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|_| P2PError::InvalidTcpConfig)?
        .with_websocket(noise::Config::new, yamux::Config::default)
        .await
        .map_err(|_| P2PError::InvalidWebsocketConfig)?
        .with_behaviour(|key| {
            Ok(BootstrapBehaviour {
                // The identify behaviour is used to share the external address and the public key with connecting clients.
                identify: identify::Behaviour::new(identify::Config::new(
                    IDENTIFY_PROTOCOL_VERSION.to_string(),
                    key.public(),
                )),
                request_response: request_response::Behaviour::new(
                    [(
                        StreamProtocol::new(BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
                kad: kad::Behaviour::new(
                    key.public().to_peer_id(),
                    kad::store::MemoryStore::new(key.public().to_peer_id()),
                ),
            })
        })
        .map_err(|_| P2PError::InvalidBehaviourConfig)?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
        .build())
}
