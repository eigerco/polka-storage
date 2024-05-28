use std::{collections::HashMap, path::PathBuf};

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
    pub path: PathBuf,
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

    /// Store deal_info in the PieceStore with key piece_cid.
    fn add_deal_for_piece(
        &self,
        piece_cid: &Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError> {
        // Check if the piece exists
        let mut piece_info = self
            .get_value_at_key(piece_cid.to_bytes(), PIECES_CF)?
            .unwrap_or_else(|| PieceInfo {
                piece_cid: *piece_cid,
                deals: Vec::new(),
            });

        // Check if deal already added for this piece
        if piece_info.deals.iter().any(|d| *d == deal_info) {
            return Err(PieceStoreError::DealExists);
        }

        // Save the new deal
        piece_info.deals.push(deal_info);
        self.put_value_at_key(piece_cid.to_bytes(), &piece_info, PIECES_CF)
    }

    /// Store the map of block_locations in the PieceStore's CidInfo store, with
    /// key piece_cid.
    ///
    /// Note: If a piece block location is already present in the CidInfo, it
    /// will be ignored.
    fn add_piece_block_locations(
        &self,
        piece_cid: &Cid,
        block_locations: &HashMap<Cid, BlockLocation>,
    ) -> Result<(), PieceStoreError> {
        for (cid, block_location) in block_locations {
            let mut info = self
                .get_value_at_key(cid.to_bytes(), CID_INFOS_CF)?
                .unwrap_or_else(|| CidInfo {
                    cid: *cid,
                    piece_block_location: Vec::new(),
                });

            if info
                .piece_block_location
                .iter()
                .any(|pbl| pbl.piece_cid == *piece_cid && pbl.location == *block_location)
            {
                continue;
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

#[cfg(test)]
mod test {
    use std::{collections::HashMap, str::FromStr};

    use cid::Cid;
    use tempfile::tempdir;

    use crate::piecestore::{
        types::{BlockLocation, DealInfo, PieceBlockLocation},
        PieceStore, PieceStoreError,
    };

    use super::{RocksDBPieceStore, RocksDBStateStoreConfig};

    fn init_database() -> RocksDBPieceStore {
        let tmp_dir = tempdir().unwrap();
        let config = RocksDBStateStoreConfig {
            path: tmp_dir.path().join("rocksdb"),
        };

        RocksDBPieceStore::new(config).unwrap()
    }

    fn cids() -> (Cid, Cid, Cid) {
        (
            Cid::from_str("QmawceGscqN4o8Y8Fv26UUmB454kn2bnkXV5tEQYc4jBd6").unwrap(),
            Cid::from_str("QmbvrHYWXAU1BuxMPNRtfeF4DS2oPmo5hat7ocqAkNPr74").unwrap(),
            Cid::from_str("QmfRL5b6fPZ851F6E2ZUWX1kC4opXzq9QDhamvU4tJGuyR").unwrap(),
        )
    }

    fn rand_deal() -> DealInfo {
        DealInfo {
            deal_id: 1,
            sector_id: 1,
            offset: 0,
            length: 100,
        }
    }

    fn block_location() -> BlockLocation {
        BlockLocation {
            rel_offset: 0,
            block_size: 100,
        }
    }

    #[test]
    fn test_piece_info_can_add_deals() {
        let store = init_database();
        let (piece_cid, piece_cid2, _) = cids();
        let deal_info = rand_deal();

        // add deal for piece
        store.add_deal_for_piece(&piece_cid, deal_info).unwrap();

        // get piece info
        let info = store.get_piece_info(&piece_cid).unwrap().unwrap();
        assert_eq!(info.deals, vec![deal_info]);

        // verify that getting a piece with a non-existent CID return None
        let info = store.get_piece_info(&piece_cid2).unwrap();
        assert!(info.is_none(), "expected None, got {:?}", info);
    }

    #[test]
    fn test_piece_adding_same_deal_twice_returns_error() {
        let store = init_database();
        let (piece_cid, _, _) = cids();
        let deal_info = rand_deal();

        // add deal for piece
        store.add_deal_for_piece(&piece_cid, deal_info).unwrap();

        // add deal for piece
        let result = store.add_deal_for_piece(&piece_cid, deal_info);
        assert!(
            matches!(result, Err(PieceStoreError::DealExists)),
            "expected error, got {:?}",
            result
        );
    }

    #[test]
    fn test_cid_info_can_add_piece_block_locations() {
        let store = init_database();
        let (piece_cid, _, _) = cids();
        let block_locations = [block_location(); 4];
        let test_cids = [
            Cid::from_str("QmW9pMY7fvbxVA2CaihgxJRzmSv15Re2TABte4HoZdfypo").unwrap(),
            Cid::from_str("QmZbaU7GGuu9F7saPgVmPSK55of8QkzEwjPrj7xxWogxiY").unwrap(),
            Cid::from_str("QmQSQYNn2K6xTDLhfNcoTjBExz5Q5gpHHBTqZZKdxsPRB9").unwrap(),
        ];

        let block_locations = test_cids
            .iter()
            .zip(block_locations.iter())
            .map(|(cid, block_location)| (*cid, *block_location))
            .collect::<HashMap<_, _>>();

        // add piece block locations
        store
            .add_piece_block_locations(&piece_cid, &block_locations)
            .unwrap();

        // get cid info
        let info = store.get_cid_info(&test_cids[0]).unwrap().unwrap();
        assert!(
            info.piece_block_location.contains(&PieceBlockLocation {
                piece_cid,
                location: block_locations[&test_cids[0]]
            }),
            "block location not found in cid info"
        );

        let info = store.get_cid_info(&test_cids[1]).unwrap().unwrap();
        assert!(
            info.piece_block_location.contains(&PieceBlockLocation {
                piece_cid,
                location: block_locations[&test_cids[1]]
            }),
            "block location not found in cid info"
        );

        let info = store.get_cid_info(&test_cids[2]).unwrap().unwrap();
        assert!(
            info.piece_block_location.contains(&PieceBlockLocation {
                piece_cid,
                location: block_locations[&test_cids[2]]
            }),
            "block location not found in cid info"
        );

        // verify that getting a piece with a non-existent CID return None
        let info = store
            .get_cid_info(&Cid::from_str("QmW9pMY7fvbxVA2CaihgxJRzmSv15Re2TABte4HoZdfypa").unwrap())
            .unwrap();
        assert!(info.is_none(), "expected None, got {:?}", info);
    }

    #[test]
    fn test_cid_info_overlapping_adds() {
        let store = init_database();
        let (piece_cid, _, _) = cids();
        let block_locations = [block_location(); 4];
        let test_cids = [
            Cid::from_str("QmW9pMY7fvbxVA2CaihgxJRzmSv15Re2TABte4HoZdfypo").unwrap(),
            Cid::from_str("QmZbaU7GGuu9F7saPgVmPSK55of8QkzEwjPrj7xxWogxiY").unwrap(),
            Cid::from_str("QmQSQYNn2K6xTDLhfNcoTjBExz5Q5gpHHBTqZZKdxsPRB9").unwrap(),
        ];

        // add piece block locations
        let locations = [
            (test_cids[0], block_locations[0]),
            (test_cids[1], block_locations[2]),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();

        store
            .add_piece_block_locations(&piece_cid, &locations)
            .unwrap();

        // add piece block locations
        let locations = [
            (test_cids[1], block_locations[1]),
            (test_cids[2], block_locations[2]),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();

        store
            .add_piece_block_locations(&piece_cid, &locations)
            .unwrap();

        // get cid info
        let info = store.get_cid_info(&test_cids[0]).unwrap().unwrap();
        assert_eq!(
            info.piece_block_location,
            vec![PieceBlockLocation {
                piece_cid,
                location: block_locations[0]
            }]
        );

        let info = store.get_cid_info(&test_cids[1]).unwrap().unwrap();
        assert_eq!(
            info.piece_block_location,
            vec![PieceBlockLocation {
                piece_cid,
                location: block_locations[1]
            }]
        );

        let info = store.get_cid_info(&test_cids[2]).unwrap().unwrap();
        assert_eq!(
            info.piece_block_location,
            vec![PieceBlockLocation {
                piece_cid,
                location: block_locations[2]
            }]
        );
    }
}
