/// PieceInfo is metadata about a piece a provider may be storing based on its
/// piece_cid -- so that, given a piece_cid during retrieval, the miner can
/// determine how to unseal it if needed
pub struct PieceInfo {
    pub piece_cid: cid::Cid,
    pub deals: Vec<DealInfo>,
}

/// DealID is an identifier for a retrieval deal (unique to a client)
type DealId = u64;

/// SectorNumber is a numeric identifier for a sector. It is usually relative to a miner.
type SectorNumber = u64;

// PaddedPieceSize is the size of a piece, in bytes
type PaddedPieceSize = u64;

/// DealInfo is information about a single deal for a given piece
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DealInfo {
    pub deal_id: DealId,
    pub sector_id: SectorNumber,
    pub offset: PaddedPieceSize,
    pub length: PaddedPieceSize,
}

impl From<&[u8]> for PieceInfo {
    fn from(value: &[u8]) -> Self {
        todo!()
    }
}

impl AsRef<[u8]> for PieceInfo {
    fn as_ref(&self) -> &[u8] {
        todo!()
    }
}
