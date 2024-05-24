use cid::Cid;
use thiserror::Error;

use self::types::{DealInfo, PieceInfo};

pub mod rocksdb;
mod types;

pub struct BlockLocation {}

pub struct CidInfo {}

pub trait PieceStore {
    /// Initialize a new store.
    fn new() -> Result<Self, PieceStoreError>
    where
        Self: Sized;

    /// Store `dealInfo` in the PieceStore with key `pieceCID`.
    fn add_deal_for_piece(
        &self,
        piece_cid: &Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError>;

    /// Store the map of blockLocations in the PieceStore's CIDInfo store, with key `pieceCID`
    fn add_piece_block_locations(
        &self,
        piece_cid: &Cid,
        block_locations: &[BlockLocation],
    ) -> Result<(), PieceStoreError>;

    /// List all piece CIDs stored in the PieceStore.
    fn list_piece_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// List all CIDInfo keys stored in the PieceStore.
    fn list_cid_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// Retrieve the [`PieceInfo`] for a given piece CID.
    fn get_piece_info(&self, piece_cid: &Cid) -> Result<PieceInfo, PieceStoreError>;

    /// Retrieve the CIDInfo for a given CID.
    fn get_cid_info(&self, payload_cid: &Cid) -> Result<CidInfo, PieceStoreError>;
}

#[derive(Debug, Error)]
pub enum PieceStoreError {
    #[error("Initialization error: {0}")]
    Initialization(String),

    #[error("Piece missing")]
    PieceMissing,

    #[error("Deal already exists")]
    DealExists,

    #[error("Failed with generic error: {0}")]
    Generic(String),
}
