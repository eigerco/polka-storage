use anyhow::anyhow;
use cid::Cid;
use libp2p::{Multiaddr, PeerId};
use primitives::p2p::{
    PeerIdRequest, PeerInfoResponse, PieceInfo, PieceInfoRequest, PieceInfoResponse,
    BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL, SP_REQUEST_RESPONSE_PROTOCOL,
};

use super::request_from_peer;

/// Creates a temporary P2P node. The node dials up the bootstrap peer and
/// requests an additional information about the storage provider identified by
/// the [`PeerId`]`.
pub async fn get_multiaddr_storage_provider(
    bootstrap_peer_id: PeerId,
    bootstrap_address: Multiaddr,
    sp_peer_id: PeerId,
) -> Result<Vec<Multiaddr>, anyhow::Error> {
    let peer_info = request_from_peer::<PeerIdRequest, PeerInfoResponse>(
        BOOTSTRAP_REQUEST_RESPONSE_PROTOCOL,
        bootstrap_peer_id,
        bootstrap_address,
        sp_peer_id.into(),
    )
    .await?;

    match peer_info {
        PeerInfoResponse::NotFound(peer) => Err(anyhow!("{:?} is not registered", peer)),
        PeerInfoResponse::Found(peer_info) => Ok(peer_info.multiaddrs),
    }
}

/// Creates a temporary P2P node. The node dials up the storage provider and
/// requests a piece information.
pub async fn get_piece_info(
    sp_peer_id: PeerId,
    sp_address: Multiaddr,
    piece_cid: Cid,
) -> Result<PieceInfo, anyhow::Error> {
    let piece_info = request_from_peer::<PieceInfoRequest, PieceInfoResponse>(
        SP_REQUEST_RESPONSE_PROTOCOL,
        sp_peer_id,
        sp_address,
        PieceInfoRequest { piece_cid },
    )
    .await?;

    match piece_info {
        PieceInfoResponse::NotFound(peer) => Err(anyhow!("{:?} piece is not found", peer)),
        PieceInfoResponse::Found(peer_info) => Ok(peer_info),
    }
}
