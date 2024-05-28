use std::collections::HashMap;

use cid::Cid;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, IteratorMode, Options, DB as RocksDB};
use serde::{de::DeserializeOwned, Serialize};

use crate::piecestore::types::{PieceBlockLocation, PieceInfo};

use super::{
    types::{BlockLocation, CidInfo, DealInfo},
    PieceStore, PieceStoreError,
};

const PIECES_CF: &str = "pieces";
const CID_INFOS_CF: &str = "cid_infos";

pub struct RocksDBStateStoreConfig {
    pub path: String,
}

/// A PieceStore implementation backed by RocksDB.
pub struct RocksDBPieceStore {
    database: RocksDB,
}

impl RocksDBPieceStore {
    /// Get the column family handle for the given column family name. Panics if
    /// the column family is not present.
    #[track_caller]
    fn cf_handle(&self, cf_name: &str) -> &ColumnFamily {
        self.database
            .cf_handle(cf_name)
            .expect("column family should be present")
    }

    fn list_cids_in_cf(&self, cf_name: &str) -> Result<Vec<Cid>, PieceStoreError> {
        let mut result = vec![];

        let iterator = self
            .database
            .iterator_cf(self.cf_handle(cf_name), IteratorMode::Start);

        for cid in iterator {
            match cid {
                Ok((key, _)) => {
                    let parsed_cid = Cid::try_from(key.as_ref()).map_err(|err| {
                        // We know that all stored CIDs are valid, so this
                        // should only happen if database is corrupted.
                        PieceStoreError::StoreSpecific(format!("invalid CID: {}", err))
                    })?;

                    result.push(parsed_cid);
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(result)
    }

    /// Get value at the specified key in the specified column family.
    fn get_value_at_key<Key, Value>(
        &self,
        key: Key,
        cf_name: &str,
    ) -> Result<Option<Value>, PieceStoreError>
    where
        Key: Into<Vec<u8>>,
        Value: DeserializeOwned,
    {
        let Some(slice) = self
            .database
            .get_pinned_cf(self.cf_handle(cf_name), key.into())?
        else {
            return Ok(None);
        };

        match ciborium::from_reader(slice.as_ref()) {
            Ok(value) => Ok(Some(value)),
            Err(err) => Err(PieceStoreError::Deserialization(err.to_string())),
        }
    }

    /// Put value at the specified key in the specified column family.
    fn put_value_at_key<Key, Value>(
        &self,
        key: Key,
        value: &Value,
        cf_name: &str,
    ) -> Result<(), PieceStoreError>
    where
        Key: AsRef<[u8]>,
        Value: Serialize,
    {
        let mut serialized = Vec::new();
        if let Err(err) = ciborium::into_writer(value, &mut serialized) {
            return Err(PieceStoreError::Serialization(err.to_string()));
        }

        self.database
            .put_cf(self.cf_handle(cf_name), key, serialized)?;

        Ok(())
    }
}

impl PieceStore for RocksDBPieceStore {
    type Config = RocksDBStateStoreConfig;

    /// Initialize a new store.
    fn new(config: Self::Config) -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        let pieces_column = ColumnFamilyDescriptor::new(PIECES_CF, Options::default());
        let cid_infos_column = ColumnFamilyDescriptor::new(CID_INFOS_CF, Options::default());

        let mut opts = Options::default();
        // Creates a new database if it doesn't exist
        opts.create_if_missing(true);
        // Create missing column families
        opts.create_missing_column_families(true);

        let database = RocksDB::open_cf_descriptors(
            &opts,
            config.path,
            vec![pieces_column, cid_infos_column],
        )?;

        Ok(Self { database })
    }

    /// Store `dealInfo` in the PieceStore with key `pieceCid`.
    fn add_deal_for_piece(
        &self,
        piece_cid: &Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError> {
        // Check if the piece exists
        let Some(mut piece_info) =
            self.get_value_at_key::<_, PieceInfo>(piece_cid.to_bytes(), PIECES_CF)?
        else {
            return Err(PieceStoreError::PieceMissing);
        };

        // Check if deal already added for this piece
        if piece_info.deals.iter().any(|d| *d == deal_info) {
            return Err(PieceStoreError::DealExists);
        }

        // Save the new deal
        piece_info.deals.push(deal_info);
        self.put_value_at_key(piece_cid.to_bytes(), &piece_info, PIECES_CF)
    }

    /// Store the map of block_locations in the PieceStore's CidInfo store, with key `piece_cid`.
    fn add_piece_block_locations(
        &self,
        piece_cid: &Cid,
        block_locations: &HashMap<Cid, BlockLocation>,
    ) -> Result<(), PieceStoreError> {
        for (cid, block_location) in block_locations {
            let Some(mut info) =
                self.get_value_at_key::<_, CidInfo>(cid.to_bytes(), CID_INFOS_CF)?
            else {
                return Err(PieceStoreError::CidInfoMissing);
            };

            if info
                .piece_block_location
                .iter()
                .any(|pbl| pbl.piece_cid == *piece_cid && pbl.location == *block_location)
            {
                return Err(PieceStoreError::BlockLocationExists(*block_location));
            }

            // Append the new block location
            info.piece_block_location.push(PieceBlockLocation {
                piece_cid: *piece_cid,
                location: *block_location,
            });

            // Save the updated CidInfo
            self.put_value_at_key(cid.to_bytes(), &info, CID_INFOS_CF)?;
        }

        Ok(())
    }

    /// List all piece CIDs stored in the PieceStore.
    fn list_piece_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError> {
        self.list_cids_in_cf(PIECES_CF)
    }

    /// List all CidInfo keys stored in the PieceStore.
    fn list_cid_info_keys(&self) -> Result<Vec<Cid>, PieceStoreError> {
        self.list_cids_in_cf(CID_INFOS_CF)
    }

    /// Retrieve the [`PieceInfo`] for a given piece CID.
    fn get_piece_info(&self, cid: &Cid) -> Result<Option<PieceInfo>, PieceStoreError> {
        self.get_value_at_key(cid.to_bytes(), PIECES_CF)
    }

    /// Retrieve the CidInfo for a given CID.
    fn get_cid_info(&self, cid: &Cid) -> Result<Option<CidInfo>, PieceStoreError> {
        self.get_value_at_key(cid.to_bytes(), CID_INFOS_CF)
    }
}

impl From<rocksdb::Error> for PieceStoreError {
    fn from(err: rocksdb::Error) -> Self {
        PieceStoreError::StoreSpecific(err.to_string())
    }
}
