use std::time::Duration;

use libp2p::{
    identify,
    identity::{Keypair, PublicKey},
    kad, noise,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
    tcp, yamux, StreamProtocol, Swarm, SwarmBuilder,
};
use libp2p_length_prefix_codec::LpCbor;
use primitives_p2p::{
    PeerIdRequest, PeerInfoResponse, BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL, IDENTIFY_PROTOCOL_VERSION,
};

pub enum Mode {
    Query,
    Server,
}

struct Config {
    mode: Mode,
    public_key: PublicKey,
}

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub identify: identify::Behaviour,
    pub request_response: request_response::Behaviour<LpCbor<PeerIdRequest, PeerInfoResponse>>,
    // TODO: replace the memory store with a persistent one and add caching
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    // TODO: add AutoNAT, this is the perfect node for that
}

impl Behaviour {
    fn with_config(config: Config) -> Self {
        let request_response = request_response::Behaviour::new(
            [(
                StreamProtocol::new(BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL),
                match config.mode {
                    Mode::Query => ProtocolSupport::Outbound,
                    Mode::Server => ProtocolSupport::Inbound,
                },
            )],
            request_response::Config::default(),
        );

        let peer_id = config.public_key.to_peer_id();
        let kad = kad::Behaviour::with_config(
            peer_id,
            kad::store::MemoryStore::new(peer_id),
            kad::Config::new(StreamProtocol::new("/polka-storage/kad/1.0.0")),
        );

        Behaviour {
            // The identify behaviour is used to share the external address and the public key with connecting clients.
            identify: identify::Behaviour::new(identify::Config::new(
                IDENTIFY_PROTOCOL_VERSION.to_string(),
                config.public_key,
            )),
            request_response,
            kad,
        }
    }

    pub async fn to_swarm(mode: Mode, keypair: Option<Keypair>) -> Swarm<Self> {
        keypair
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
            .expect("static config should be correct")
            .with_websocket(noise::Config::new, yamux::Config::default)
            .await
            .expect("static config should be correct")
            .with_behaviour(|key| {
                let config = Config {
                    mode,
                    public_key: key.public(),
                };
                Ok(Behaviour::with_config(config))
            })
            .expect("static config should be correct")
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10)))
            .build()
    }
}
