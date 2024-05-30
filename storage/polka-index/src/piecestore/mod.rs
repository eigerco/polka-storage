use std::collections::HashMap;

use cid::Cid;
use rocksdb::RocksDBError;
use thiserror::Error;

use self::types::{BlockLocation, CidInfo, DealInfo, PieceInfo};

pub mod rocksdb;
pub mod types;

pub trait PieceStore {
    /// Implementation-specific configuration.
    type Config;

    /// Initialize a new store.
    fn new(config: Self::Config) -> Result<Self, PieceStoreError>
    where
        Self: Sized;

    /// Store [`DealInfo`] in the PieceStore with key piece [`Cid`].
    fn add_deal_for_piece(
        &self,
        piece_cid: &Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError>;

    /// Store the map of [`BlockLocation`] in the [`PieceStore`]'s [`CidInfo`] store, with
    /// key piece [`Cid`].
    ///
    /// Note: If a piece block location is already present in the [`CidInfo`], it
    /// will be ignored.
    fn add_piece_block_locations(
        &self,
        piece_cid: &Cid,
        block_locations: &HashMap<Cid, BlockLocation>,
    ) -> Result<(), PieceStoreError>;

    /// List all piece [`Cid`]s stored in the [`PieceStore`].
    fn list_piece_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// List all [`CidInfo`]s keys stored in the [`PieceStore`].
    fn list_cid_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// Retrieve the [`PieceInfo`] for a given piece [`Cid`].
    fn get_piece_info(&self, cid: &Cid) -> Result<Option<PieceInfo>, PieceStoreError>;

    /// Retrieve the [`CidInfo`] associated with piece [`Cid`].
    fn get_cid_info(&self, cid: &Cid) -> Result<Option<CidInfo>, PieceStoreError>;
}

/// Error that can occur when interacting with the [`PieceStore`].
#[derive(Debug, Error)]
pub enum PieceStoreError {
    #[error("Initialization error: {0}")]
    Initialization(String),

    #[error("Deal already exists")]
    DealExists,

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error(transparent)]
    StoreError(#[from] RocksDBError),
}
