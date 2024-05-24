use cid::Cid;
use thiserror::Error;

pub struct DealInfo {}

pub struct BlockLocation {}

pub struct PieceInfo {}

pub struct CidInfo {}

pub struct PieceStore<T> {
    pieces: T,
    cid_infos: T,
}

impl<T> PieceStore<T>
where
    T: StateStore,
{
    pub fn new() -> Result<Self, PieceStoreError> {
        let pieces = StateStore::init(todo!())?;
        let cid_infos = StateStore::init(todo!())?;

        Ok(Self { pieces, cid_infos })
    }

    /// Store `dealInfo` in the PieceStore with key `pieceCID`.
    pub fn add_deal_for_piece(
        &self,
        piece_cid: &Cid,
        deal_info: &DealInfo,
    ) -> Result<(), PieceStoreError> {
        todo!()
    }

    /// Store the map of blockLocations in the PieceStore's CIDInfo store, with key `pieceCID`
    pub fn add_piece_block_locations(
        &self,
        piece_cid: &Cid,
        block_locations: &[BlockLocation],
    ) -> Result<(), PieceStoreError> {
        todo!()
    }

    /// List all piece CIDs stored in the PieceStore.
    pub fn list_piece_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError> {
        todo!()
    }

    /// List all CIDInfo keys stored in the PieceStore.
    pub fn list_cid_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError> {
        todo!()
    }

    /// Retrieve the [`PieceInfo`] for a given piece CID.
    pub fn get_piece_info(&self, piece_cid: &Cid) -> Result<PieceInfo, PieceStoreError> {
        todo!()
    }

    /// Retrieve the CIDInfo for a given CID.
    pub fn get_cid_info(&self, payload_cid: &Cid) -> Result<CidInfo, PieceStoreError> {
        todo!()
    }
}

/// Trait for a state store that can be used by the PieceStore.
pub trait StateStore {
    /// Initialize the state store at `path`.
    fn init(path: &str) -> Result<Self, PieceStoreError>
    where
        Self: Sized;

    /// List all keys stored in the state store.
    fn list(&self) -> Result<Vec<Cid>, PieceStoreError>;

    /// Retrieve the value stored at `cid`.
    fn get(&self, cid: &Cid) -> Result<Vec<u8>, PieceStoreError>;

    /// Check if the state store has a value stored at `cid`.
    fn has(&self, cid: &Cid) -> Result<bool, PieceStoreError>;
}

#[derive(Debug, Error)]
pub enum PieceStoreError {
    #[error("Initialization error: {0}")]
    Initialization(String),
}

struct RocksDB {
    db: rocksdb::DB,
}

impl StateStore for RocksDB {
    fn init(path: &str) -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        let db = rocksdb::DB::open_default(path).map_err(|e| {
            PieceStoreError::Initialization(format!("failed to open RocksDB: {}", e))
        })?;

        Ok(Self { db })
    }

    fn list(&self) -> Result<Vec<Cid>, PieceStoreError> {
        todo!()
    }

    fn get(&self, cid: &Cid) -> Result<Vec<u8>, PieceStoreError> {
        todo!()
    }

    fn has(&self, cid: &Cid) -> Result<bool, PieceStoreError> {
        todo!()
    }
}
