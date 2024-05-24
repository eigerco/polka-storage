use crate::piecestore::types::PieceInfo;

use super::{PieceStore, PieceStoreError};

pub struct RocksDBPieceStore {
    pieces: rocksdb::DB,
    cid_infos: rocksdb::DB,
}

impl PieceStore for RocksDBPieceStore {
    fn new() -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        let pieces = rocksdb::DB::open_default("").map_err(|e| {
            PieceStoreError::Initialization(format!("failed to open RocksDB: {}", e))
        })?;

        let cid_infos = rocksdb::DB::open_default("").map_err(|e| {
            PieceStoreError::Initialization(format!("failed to open RocksDB: {}", e))
        })?;

        Ok(Self { pieces, cid_infos })
    }

    fn add_deal_for_piece(
        &self,
        piece_cid: &cid::Cid,
        deal_info: super::DealInfo,
    ) -> Result<(), PieceStoreError> {
        // Check if the piece exists
        let Some(piece_info) = self.pieces.get_pinned(piece_cid.to_bytes())? else {
            return Err(PieceStoreError::PieceMissing);
        };
        let mut piece_info: PieceInfo = piece_info.as_ref().into();

        // Check if deal already added
        if !piece_info.deals.iter().any(|d| *d == deal_info) {
            return Err(PieceStoreError::DealExists);
        }

        // Save the new deal
        piece_info.deals.push(deal_info);
        self.pieces.put(piece_cid.to_bytes(), piece_info.as_ref())?;

        Ok(())
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

impl From<rocksdb::Error> for PieceStoreError {
    fn from(err: rocksdb::Error) -> Self {
        // TODO: map rocksdb errors to PieceStoreError
        PieceStoreError::Generic(err.to_string())
    }
}
