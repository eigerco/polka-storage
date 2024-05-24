use super::{PieceStore, PieceStoreError};

pub struct RocksDBPieceStore {
    // pieces: rocksdb::DB,
    // cid_infos: rocksdb::DB,
}

impl PieceStore for RocksDBPieceStore {
    fn new() -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        // let pieces = rocksdb::DB::open_default("").map_err(|e| {
        //     PieceStoreError::Initialization(format!("failed to open RocksDB: {}", e))
        // })?;

        // let cid_infos = rocksdb::DB::open_default("").map_err(|e| {
        //     PieceStoreError::Initialization(format!("failed to open RocksDB: {}", e))
        // })?;

        Ok(Self {})
    }

    fn add_deal_for_piece(
        &self,
        piece_cid: &cid::Cid,
        deal_info: &super::DealInfo,
    ) -> Result<(), PieceStoreError> {
        todo!()
    }

    fn add_piece_block_locations(
        &self,
        piece_cid: &cid::Cid,
        block_locations: &[super::BlockLocation],
    ) -> Result<(), PieceStoreError> {
        todo!()
    }

    fn list_piece_info_keys(&self) -> Result<Vec<cid::Cid>, PieceStoreError> {
        todo!()
    }

    fn list_cid_info_keys(&self) -> Result<Vec<cid::Cid>, PieceStoreError> {
        todo!()
    }

    fn get_piece_info(&self, piece_cid: &cid::Cid) -> Result<super::PieceInfo, PieceStoreError> {
        todo!()
    }

    fn get_cid_info(&self, payload_cid: &cid::Cid) -> Result<super::CidInfo, PieceStoreError> {
        todo!()
    }
}
