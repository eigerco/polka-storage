use std::collections::HashMap;

use cid::Cid;
use thiserror::Error;

use self::types::{BlockLocation, CidInfo, DealInfo, PieceInfo};

pub mod rocksdb;
mod types;

pub trait PieceStore {
    /// Implementation-specific configuration.
    type Config;

    /// Initialize a new store.
    fn new(config: Self::Config) -> Result<Self, PieceStoreError>
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
        block_locations: &HashMap<Cid, BlockLocation>,
    ) -> Result<(), PieceStoreError>;

    /// List all piece CIDs stored in the PieceStore.
    fn list_piece_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// List all CIDInfo keys stored in the PieceStore.
    fn list_cid_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// Retrieve the PieceInfo for a given piece CID.
    fn get_piece_info(&self, cid: &Cid) -> Result<Option<PieceInfo>, PieceStoreError>;

    /// Retrieve the CidInfo associated with piece CID.
    fn get_cid_info(&self, cid: &Cid) -> Result<Option<CidInfo>, PieceStoreError>;
}

/// Error that can occur when interacting with the PieceStore.
#[derive(Debug, Error)]
pub enum PieceStoreError {
    #[error("Initialization error: {0}")]
    Initialization(String),

    #[error("Piece missing")]
    PieceMissing,

    #[error("CidInfo missing")]
    CidInfoMissing,

    #[error("Deal already exists")]
    DealExists,

    #[error("Block location already exists: {0:?}")]
    BlockLocationExists(BlockLocation),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Failed with store specific error: {0}")]
    StoreSpecific(String),
}
