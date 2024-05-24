use cid::Cid;
use rocksdb::DB as RocksDB;
use thiserror::Error;

struct DealInfo {}

struct BlockLocation {}

struct PieceInfo {}

struct CidInfo {}

pub struct PieceStore<StateStore> {
    pieces: StateStore,
    cid_infos: StateStore,
}

impl<StateStore> PieceStore<StateStore> {
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
trait StateStore {
    fn list(&self) -> Result<Vec<Cid>, PieceStoreError>;
    fn get(&self, cid: &Cid) -> Result<Vec<u8>, PieceStoreError>;
    fn has(&self, cid: &Cid) -> Result<bool, PieceStoreError>;
}

#[derive(Debug, Error)]
pub enum PieceStoreError {}

impl StateStore for RocksDB {
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
