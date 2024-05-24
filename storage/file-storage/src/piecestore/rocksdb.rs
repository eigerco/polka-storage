use rocksdb::{ColumnFamilyDescriptor, Options};

use crate::piecestore::types::PieceInfo;

use super::{PieceStore, PieceStoreError};

const PIECES_CF: &str = "pieces";
const CID_INFOS_CF: &str = "cid_infos";

/// A PieceStore implementation backed by RocksDB.
pub struct RocksDBPieceStore {
    database: rocksdb::DB,
}

impl RocksDBPieceStore {
    /// Get the column family handle for the given column family name. Panics if
    /// the column family is not present.
    #[track_caller]
    fn cf_handle(&self, cf_name: &str) -> &rocksdb::ColumnFamily {
        self.database
            .cf_handle(cf_name)
            .expect("column family should be present")
    }
}

impl PieceStore for RocksDBPieceStore {
    /// Initialize a new store.
    fn new(path: &str) -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        let pieces_column = ColumnFamilyDescriptor::new(PIECES_CF, Options::default());
        let cid_infos_column = ColumnFamilyDescriptor::new("cid_infos", Options::default());

        let mut opts = Options::default();
        // Creates a new database if it doesn't exist
        opts.create_if_missing(true);
        // Create missing column families
        opts.create_missing_column_families(true);

        let database =
            rocksdb::DB::open_cf_descriptors(&opts, path, vec![pieces_column, cid_infos_column])?;

        Ok(Self { database })
    }

    /// Store `dealInfo` in the PieceStore with key `pieceCID`.
    fn add_deal_for_piece(
        &self,
        piece_cid: &cid::Cid,
        deal_info: super::DealInfo,
    ) -> Result<(), PieceStoreError> {
        // Check if the piece exists
        let Some(piece_info) = self
            .database
            .get_pinned_cf(self.cf_handle(PIECES_CF), piece_cid.to_bytes())?
        else {
            return Err(PieceStoreError::PieceMissing);
        };
        let mut piece_info: PieceInfo = piece_info.as_ref().into();

        // Check if deal already added
        if piece_info.deals.iter().any(|d| *d == deal_info) {
            return Err(PieceStoreError::DealExists);
        }

        // Save the new deal
        piece_info.deals.push(deal_info);
        self.database.put_cf(
            self.cf_handle(PIECES_CF),
            piece_cid.to_bytes(),
            piece_info.as_ref(),
        )?;

        Ok(())
    }

    /// Store the map of blockLocations in the PieceStore's CIDInfo store, with key `pieceCID`
    fn add_piece_block_locations(
        &self,
        piece_cid: &cid::Cid,
        block_locations: &[super::BlockLocation],
    ) -> Result<(), PieceStoreError> {
        todo!()
    }

    /// List all piece CIDs stored in the PieceStore.
    fn list_piece_info_keys(&self) -> Result<Vec<cid::Cid>, PieceStoreError> {
        todo!()
    }

    /// List all CIDInfo keys stored in the PieceStore.
    fn list_cid_info_keys(&self) -> Result<Vec<cid::Cid>, PieceStoreError> {
        todo!()
    }

    /// Retrieve the [`PieceInfo`] for a given piece CID.
    fn get_piece_info(&self, piece_cid: &cid::Cid) -> Result<super::PieceInfo, PieceStoreError> {
        todo!()
    }

    /// Retrieve the CIDInfo for a given CID.
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
