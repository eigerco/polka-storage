use cid::Cid;
use serde::{Deserialize, Serialize};

/// Metadata about a piece that provider may be storing based on its [`Cid`]. So
/// that, given a [`Cid`] during retrieval, the miner can determine how to
/// unseal it if needed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceInfo {
    pub piece_cid: Cid,
    pub deals: Vec<DealInfo>,
}

/// Identifier for a retrieval deal (unique to a client)
type DealId = u64;

/// Numeric identifier for a sector. It is usually relative to a miner.
type SectorNumber = u64;

/// Information about a single deal for a given piece
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealInfo {
    pub deal_id: DealId,
    pub sector_id: SectorNumber,
    pub offset: u64,
    pub length: u64,
}

/// Information about where a given block is relative to the overall piece
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockLocation {
    pub rel_offset: u64,
    pub block_size: u64,
}

/// Contains block information along with the [`Cid`] of the piece the block is
/// inside of
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PieceBlockLocation {
    pub piece_cid: Cid,
    pub location: BlockLocation,
}

/// Information about where a given [`Cid`] will live inside a piece
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CidInfo {
    pub cid: Cid,
    pub piece_block_location: Vec<PieceBlockLocation>,
}
