use std::{fmt::Debug, time::Duration};

use anyhow::bail;
use futures::StreamExt;
use libp2p::{
    noise,
    request_response::{
        self, cbor::Behaviour as ReqRespBehaviour, Event as ReqRespEvent, Message, ProtocolSupport,
    },
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use libp2p_swarm::{NetworkBehaviour, SwarmEvent};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{info, instrument};

pub mod resolvers;

/// Creates a temporary P2P node. The node then connects to the specified peer
/// and submits a request. The future resolves when the response is received or
/// the error is observed.
#[instrument]
pub(crate) async fn request_from_peer_sync<Req, Resp>(
    protocol: &'static str,
    (peer_id, peer_multiaddr): (PeerId, Multiaddr),
    request: Req,
) -> Result<Resp, anyhow::Error>
where
    Req: Debug + Send + Serialize + DeserializeOwned + 'static,
    Resp: Debug + Send + Serialize + DeserializeOwned + 'static,
{
    let behaviour = ReqRespBehaviour::<Req, Resp>::new(
        [(StreamProtocol::new(protocol), ProtocolSupport::Full)],
        request_response::Config::default(),
    );

    let mut swarm = new_swarm(behaviour)?;

    // Add known external peer to the swarm. This peer is autodialed before the
    // request is published. If the peer can't be dialed the
    // `ReqRespEvent::OutboundFailure` is received.
    swarm.add_peer_address(peer_id, peer_multiaddr);

    // Send request to the peer
    swarm.behaviour_mut().send_request(&peer_id, request);

    // Wait for the response
    wait_response(swarm).await
}

/// Pull the swarm until we receive the response from the peer or an error is observed.
#[instrument(skip_all)]
async fn wait_response<Req, Resp>(
    mut swarm: Swarm<ReqRespBehaviour<Req, Resp>>,
) -> Result<Resp, anyhow::Error>
where
    Req: Debug + Send + Serialize + DeserializeOwned,
    Resp: Debug + Send + Serialize + DeserializeOwned,
{
    loop {
        let event = swarm.select_next_some().await;

        if let SwarmEvent::Behaviour(event) = event {
            match event {
                ReqRespEvent::Message { message, .. } => {
                    if let Message::Response { response, .. } = message {
                        info!(?response, "Received response");
                        return Ok(response);
                    }
                }
                ReqRespEvent::OutboundFailure { error, .. } => {
                    bail!(error)
                }
                _ => {}
            }
        }
    }
}

pub fn new_swarm<B>(behaviour: B) -> Result<Swarm<B>, noise::Error>
where
    B: NetworkBehaviour,
{
    Ok(SwarmBuilder::with_new_identity()
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
