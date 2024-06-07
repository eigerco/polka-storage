// The name of this file is `rdb.rs` to avoid clashing with the `rocksdb` import.
use std::{collections::HashMap, path::PathBuf, str::FromStr};

use cid::{multihash::Multihash, Cid, CidGeneric};
use integer_encoding::{VarInt, VarIntReader};
use rocksdb::{
    AsColumnFamilyRef, ColumnFamily, ColumnFamilyDescriptor, IteratorMode, Options,
    WriteBatchWithTransaction, DB as RocksDB,
};
use serde::{de::DeserializeOwned, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    ext::WriteBatchWithTransactionExt, CarIndexRecord, DealInfo, FlaggedMetadata, FlaggedPiece,
    FlaggedPiecesListFilter, OffsetSize, PieceInfo, PieceStoreError, Record, Service,
};

// NOTE(@jmg-duarte,04/06/2024): In the LevelDB implementation these CFs are numbered prefixes instead.
// I've decided to use a string for legibility, but if it affects performance, let's switch.

// NOTE(@jmg-duarte,04/06/2024): We probably could split the interface according to the respective column family

/// Key for the next free cursor.
///
/// This is not a column family as in the original source code it is not a prefix.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L30-L32>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L54-L56>
const NEXT_CURSOR_KEY: &str = "next_cursor";

/// Column family name to store the mapping between a [`Cid`] and its cursor.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L34-L37>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L58-L61>
const PIECE_CID_TO_CURSOR_CF: &str = "piece_cid_to_cursor";

/// Column family name to store the mapping between [`Multihash`]es and piece [`Cid`]s.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L39-L42>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L62-L64>
const MULTIHASH_TO_PIECE_CID_CF: &str = "multihash_to_piece_cids";

/// Column family name to store the flagged piece [`Cid`]s.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L44-L47>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L66-L68>
const PIECE_CID_TO_FLAGGED_CF: &str = "piece_cid_to_flagged";

// TODO(@jmg-duarte,04/06/2024): double check and document
const CURSOR_TO_OFFSET_SIZE_CF: &str = "cursor_to_offset_size";

fn key_cursor_prefix(cursor: u64) -> String {
    format!("{}/", cursor)
}

pub struct RocksDBStateStoreConfig {
    pub path: PathBuf,
}

/// A [`super::PieceStore`] implementation backed by RocksDB.
pub struct RocksDBPieceStore {
    database: RocksDB,
}

// TODO(@jmg-duarte,05/06/2024): review all CF usages

impl RocksDBPieceStore {
    // TODO(@jmg-duarte,05/06/2024): refactor the API to take the CF first (just makes more sense)

    fn new(config: RocksDBStateStoreConfig) -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        let column_families = [
            PIECE_CID_TO_FLAGGED_CF,
            PIECE_CID_TO_CURSOR_CF,
            CURSOR_TO_OFFSET_SIZE_CF,
        ]
        .into_iter()
        .map(|cf| ColumnFamilyDescriptor::new(cf, Options::default()));

        let mut opts = Options::default();
        // Creates a new database if it doesn't exist
        opts.create_if_missing(true);
        // Create missing column families
        opts.create_missing_column_families(true);

        let database = RocksDB::open_cf_descriptors(&opts, config.path, column_families)?;

        Ok(Self { database })
    }

    /// Get the column family handle for the given column family name. Panics if
    /// the column family is not present. The column families needed and used
    /// are created at initialization. They will always be present.
    #[track_caller]
    fn cf_handle(&self, cf_name: &str) -> &ColumnFamily {
        self.database
            .cf_handle(cf_name)
            .expect("column family should have been initialized")
    }

    fn remove_value_at_key<Key>(&self, key: Key, cf_name: &str) -> Result<(), PieceStoreError>
    where
        Key: AsRef<[u8]>,
    {
        Ok(self.database.delete_cf(self.cf_handle(cf_name), key)?)
    }

    /// Get value at the specified key in the specified column family.
    fn get_value_at_key<Key, Value>(
        &self,
        key: Key,
        cf_name: &str,
    ) -> Result<Option<Value>, PieceStoreError>
    where
        Key: AsRef<[u8]>,
        Value: DeserializeOwned,
    {
        let Some(slice) = self.database.get_pinned_cf(self.cf_handle(cf_name), key)? else {
            return Ok(None);
        };

        match ciborium::from_reader(slice.as_ref()) {
            Ok(value) => Ok(Some(value)),
            // ciborium error is bubbled up as a string because it is generic
            // and we didn't want to add a generic type to the PieceStoreError
            Err(err) => Err(PieceStoreError::Deserialization(err.to_string())),
        }
    }

    /// Serializes the `Value` to CBOR and puts resulting bytes at the specified key in the specified column family.
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
            // ciborium error is bubbled up as a string because it is generic
            // and we didn't want to add a generic type to the PieceStoreError
            return Err(PieceStoreError::Serialization(err.to_string()));
        }

        Ok(self
            .database
            .put_cf(self.cf_handle(cf_name), key, serialized)?)
    }

    fn get_piece_cid_to_metadata(
        &self,
        piece_cid: Cid,
    ) -> Result<Option<PieceInfo>, PieceStoreError> {
        self.get_value_at_key(piece_cid.to_bytes(), PIECE_CID_TO_CURSOR_CF)
    }

    fn set_piece_cid_to_metadata(
        &self,
        piece_cid: Cid,
        metadata: &PieceInfo,
    ) -> Result<(), PieceStoreError> {
        self.put_value_at_key(piece_cid.to_bytes(), metadata, PIECE_CID_TO_CURSOR_CF)
    }

    fn set_multihashes_to_piece_cid(
        &self,
        records: &Vec<CarIndexRecord>,
        piece_cid: Cid,
    ) -> Result<(), PieceStoreError> {
        // NOTE(@jmg-duarte,06/06/2024): maybe this should be a transaction? The Go docs indicate otherwise though
        // https://github.com/ipfs/go-datastore/blob/1de47089f5c72b61d91b5cd9043e49fe95771ac0/datastore.go#L97-L106
        let mut batch = WriteBatchWithTransaction::<false>::default();

        for record in records {
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L166-L167
            let multihash_key = format!("{:x?}", record.cid.hash().to_bytes());
            let cids = if let Some(mut cids) =
                self.get_value_at_key::<_, Vec<Cid>>(&multihash_key, MULTIHASH_TO_PIECE_CID_CF)?
            {
                if cids.contains(&piece_cid) {
                    continue;
                }
                cids.push(piece_cid);
                cids
            } else {
                vec![piece_cid]
            };
            batch.put_cf_cbor(
                self.cf_handle(MULTIHASH_TO_PIECE_CID_CF),
                multihash_key,
                &cids,
            )?;
        }
        // "commit" the batch, should be equivalent to
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L216-L218
        Ok(self.database.write(batch)?)
    }

    /// Get the next available cursor.
    ///
    /// Returns [`PieceStoreError::NotFoundError`] if no cursor has been set.
    /// Use [`Self::set_next_cursor`] to set the next cursor.
    ///
    /// Source:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L109-L118>
    fn get_next_cursor(&self) -> Result<(u64, String), PieceStoreError> {
        let pinned_slice = self.database.get_pinned(NEXT_CURSOR_KEY)?;
        let Some(pinned_slice) = pinned_slice else {
            // In most places the original source code has some special handling for the missing key,
            // however, that does not apply for a missing cursor
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L391-L396
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L111-L114
            return Err(PieceStoreError::NotFoundError);
        };

        // We use varint instead of cborium here to match the original implementation
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L116
        let cursor = pinned_slice.as_ref().read_varint::<u64>()?;
        Ok((cursor, key_cursor_prefix(cursor)))
    }

    /// Set the next available cursor.
    ///
    /// Source:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L124-L130>
    fn set_next_cursor(&self, cursor: u64) -> Result<(), PieceStoreError> {
        let encoded_cursor = cursor.encode_var_vec();
        Ok(self.database.put(NEXT_CURSOR_KEY, encoded_cursor)?)
    }

    /// Add a [`Record`] to the database under a given cursor prefix
    fn add_index_record(&self, cursor_prefix: &str, record: Record) -> Result<(), PieceStoreError> {
        let key = format!("{}{:x?}", cursor_prefix, record.cid.hash().to_bytes());
        Ok(self.database.put(key, record.offset_size.to_bytes())?)
    }
}

// NOTE(@jmg-duarte,06/06/2024): this abstraction is not ported with 100% fidelity to the Go solution
// In the Go solution a double interface is used, the first is the `Service` interface, the second is
// the `datastore.Batching` interface. The first one is implemented by a generic `DB` structure and
// the second is implemented by the actual database, be it Yugabyte or LevelDB. I suspect this is due
// to the fact that Go (like Rust) doesn't have inheritance and promotes composition, but the latter
// "acts" a bit differently in both languages.

impl Service for RocksDBPieceStore {
    fn add_deal_for_piece(
        &self,
        piece_cid: Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError> {
        // Check if the piece exists
        let mut piece_info = self
            .get_piece_cid_to_metadata(piece_cid)?
            .unwrap_or_else(|| PieceInfo::with_cid(piece_cid));

        // Check if deal already added for this piece
        if let Some(deal) = piece_info.deals.iter().find(|d| **d == deal_info) {
            return Err(PieceStoreError::DealExists(deal.deal_uuid));
        }

        // Save the new deal
        piece_info.deals.push(deal_info);
        self.set_piece_cid_to_metadata(piece_cid, &piece_info)
    }

    // In Boost, this operation is performed by running a goroutine that will feed the returned channel,
    // in Rust we there's a mix of things that make life especially difficult for us here, however,
    // since the whole Service relies on the sync API of RocksDB, we can just use `tokio.spawn_blocking`
    // and ensure the user is aware that this operation takes a while to complete. Same for `get_index`.
    fn add_index(
        &self,
        piece_cid: Cid,
        records: Vec<Record>,
        is_complete_index: bool,
    ) -> Result<(), PieceStoreError> {
        let car_index_records = records
            .iter()
            .cloned()
            .map(CarIndexRecord::from)
            .collect::<Vec<_>>();

        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L369-L374
        self.set_multihashes_to_piece_cid(&car_index_records, piece_cid)?;

        let (mut metadata, cursor_prefix) =
            // This looks a bit strange at first but in Go mutability is much more of a thing than in Rust, hence,
            // you get a bunch of parts where a variable is declared (and initialized) to be overwritten in a deeper scope
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L376-L410
            if let Some(metadata) = self.get_piece_cid_to_metadata(piece_cid)? {
                let cursor_prefix = key_cursor_prefix(metadata.cursor);
                (metadata, cursor_prefix)
            } else {
                let mut metadata = PieceInfo::with_cid(piece_cid);
                let (next_cursor, next_cursor_prefix) = self.get_next_cursor()?;
                self.set_next_cursor(next_cursor + 1)?;

                metadata.cursor = next_cursor;
                metadata.complete_index = is_complete_index;
                (metadata, next_cursor_prefix)
            };

        records
            .into_iter()
            .map(|record| self.add_index_record(&cursor_prefix, record))
            .collect::<Result<_, _>>()?;

        metadata.indexed_at = time::OffsetDateTime::now_utc();
        self.set_piece_cid_to_metadata(piece_cid, &metadata)
    }

    fn get_index(&self, piece_cid: Cid) -> Result<Vec<Record>, PieceStoreError> {
        let Some(metadata) = self.get_piece_cid_to_metadata(piece_cid)? else {
            return Err(PieceStoreError::NotFoundError);
        };

        // This is equivalent to `db.AllRecords`
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L304-L349
        let cursor_prefix = format!("{}/", metadata.cursor);

        // NOTE(@jmg-duarte,06/06/2024): Not sure if we can do this without setting extra stuff on DB creation
        // https://github.com/facebook/rocksdb/wiki/Prefix-Seek
        // TODO(@jmg-duarte,06/06/2024): review usage patterns as we can place all cursors for this in a single column family
        let iterator = self.database.prefix_iterator(&cursor_prefix);

        let mut records = vec![];
        for it in iterator {
            let (key, value) = it?;
            // With some trickery, we can probably get rid of this allocation
            let key = String::from_utf8(key.to_vec())?
                // I suspect there might be an off-by-1 error here
                .split_off(cursor_prefix.len() + 1);
            let mh_bytes = hex::decode(&key)?;
            let mh = Multihash::<64>::from_bytes(&mh_bytes)?;
            let cid = Cid::new_v1(0x55, mh); // TODO(@jmg-duarte,06/06/2024): make the CID code const
            let offset_size = OffsetSize::from_bytes(&value)?;
            records.push(Record { cid, offset_size });
        }

        // The main difference here is that we don't need to return IndexRecord
        // since we're not sending the records over a channel, we should be able to
        // just error out as soon as we hit one
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L285-L289

        Ok(records)
    }

    fn is_indexed(&self, piece_cid: Cid) -> Result<bool, PieceStoreError> {
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            // If the piece does not exist, it's clearly not indexed
            .map_or(false, |piece_info: PieceInfo| {
                // The sentinel value we're using is the Unix epoch, so we check against that
                piece_info.indexed_at == OffsetDateTime::UNIX_EPOCH
            }))
    }

    fn is_complete_index(&self, piece_cid: Cid) -> Result<bool, PieceStoreError> {
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            .map_or(false, |piece_info: PieceInfo| piece_info.complete_index))
    }

    fn get_offset_size(
        &self,
        piece_cid: Cid,
        multihash: Multihash<64>,
    ) -> Result<OffsetSize, PieceStoreError> {
        let cursor = self
            .get_piece_cid_to_metadata(piece_cid)?
            .map(|piece_info: PieceInfo| piece_info.cursor)
            .ok_or(PieceStoreError::NotFoundError)?;

        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L164-L165
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L370-L371
        self.get_value_at_key(
            // Multihash.String is implemented as hex format
            // https://github.com/multiformats/go-multihash/blob/728cc45bec837e8ff5abc3ca3f46bcec52b563d2/multihash.go#L177-L185
            format!("{}/{:x?}", cursor, multihash.to_bytes()),
            CURSOR_TO_OFFSET_SIZE_CF,
        )?
        .ok_or(PieceStoreError::NotFoundError)
    }

    fn list_pieces(&self) -> Result<Vec<Cid>, PieceStoreError> {
        let iterator = self
            .database
            .iterator_cf(self.cf_handle(PIECE_CID_TO_CURSOR_CF), IteratorMode::Start);

        iterator
            .map(|line| {
                let (key, _) = line?;

                let parsed_cid = Cid::try_from(key.as_ref()).map_err(|err| {
                    // We know that all stored CIDs are valid, so this
                    // should only happen if database is corrupted.
                    PieceStoreError::Deserialization(format!("invalid CID: {}", err))
                })?;

                Ok(parsed_cid)
            })
            .collect()
    }

    // TODO(@jmg-duarte,05/06/2024): improve docs
    /// Get a Piece's Metadata. If none is found [`PieceStoreError::NotFoundError`] is returned.
    fn get_piece_metadata(&self, piece_cid: Cid) -> Result<PieceInfo, PieceStoreError> {
        self.get_piece_cid_to_metadata(piece_cid)
            .and_then(|piece_info: Option<PieceInfo>| {
                piece_info.ok_or(PieceStoreError::NotFoundError)
            })
    }

    fn get_piece_deals(&self, piece_cid: Cid) -> Result<Vec<DealInfo>, PieceStoreError> {
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            .map(|piece_info: PieceInfo| piece_info.deals)
            .ok_or(PieceStoreError::NotFoundError)?)
    }

    fn indexed_at(&self, piece_cid: Cid) -> Result<time::OffsetDateTime, PieceStoreError> {
        // The Go implementation returns the "epoch" but returning the error makes more sense
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L461-L468
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            .map(|piece_info: PieceInfo| piece_info.indexed_at)
            .ok_or(PieceStoreError::NotFoundError)?)
    }

    fn pieces_containing_multihash(
        &self,
        multihash: Multihash<64>,
    ) -> Result<Vec<Cid>, PieceStoreError> {
        let iterator = self.database.iterator_cf(
            self.cf_handle(MULTIHASH_TO_PIECE_CID_CF),
            IteratorMode::Start,
        );

        iterator
            .map(|line| {
                let (key, value) = line?;
                let mh: Multihash<64> = Multihash::from_bytes(&key)?;
                let parsed_cid = Cid::try_from(value.as_ref()).map_err(|err| {
                    // We know that all stored CIDs are valid, so this
                    // should only happen if database is corrupted.
                    PieceStoreError::Deserialization(format!("invalid CID: {}", err))
                })?;
                Ok((mh, parsed_cid))
            })
            .filter_map(|res| match res {
                Ok((mh, cid)) if mh == multihash => Some(Ok(cid)),
                Ok(_) => None,
                Err(err) => Some(Err(err)),
            })
            .collect()
    }

    fn remove_deal_for_piece(
        &self,
        piece_cid: Cid,
        deal_uuid: Uuid,
    ) -> Result<(), PieceStoreError> {
        let mut piece_info = self.get_piece_metadata(piece_cid)?;

        if let Some((idx, _)) = piece_info
            .deals
            .iter()
            .enumerate()
            .filter(|(_, deal)| deal.deal_uuid == deal_uuid)
            .next()
        {
            piece_info.deals.remove(idx);
        }

        if piece_info.deals.is_empty() {
            self.remove_piece_metadata(piece_cid)?;
            return Ok(());
        }

        self.put_value_at_key(piece_cid.to_bytes(), &piece_info, PIECE_CID_TO_CURSOR_CF)
    }

    // TODO(@jmg-duarte,06/06/2024): double check
    fn remove_piece_metadata(&self, piece_cid: Cid) -> Result<(), PieceStoreError> {
        // Remove all the multihashes before, as without metadata, they're useless.
        // This operation is made first for consistency — i.e. if this fails
        // For more details, see the original implementation:
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L610-L615
        self.remove_indexes(piece_cid)?;
        self.remove_value_at_key(piece_cid.to_bytes(), PIECE_CID_TO_CURSOR_CF)
    }

    fn remove_indexes(&self, piece_cid: Cid) -> Result<(), PieceStoreError> {
        let Some(metadata) = self.get_piece_cid_to_metadata(piece_cid)? else {
            return Err(PieceStoreError::NotFoundError);
        };

        let cursor_prefix = format!("{}/", metadata.cursor);
        let iterator = self.database.prefix_iterator(&cursor_prefix);
        let mut batch = WriteBatchWithTransaction::<false>::default();

        for it in iterator {
            let (key, _) = it?;
            // Possible off-by-one error
            let (_, mh_key) = key.split_at(cursor_prefix.len() + 1);
            let Some(mut cids) =
                self.get_value_at_key::<_, Vec<Cid>>(mh_key, MULTIHASH_TO_PIECE_CID_CF)?
            else {
                return Err(PieceStoreError::NotFoundError);
            };

            let Some(idx) = cids.iter().position(|cid| cid == &piece_cid) else {
                continue;
            };

            if cids.is_empty() {
                self.database.delete_cf(
                    self.cf_handle(MULTIHASH_TO_PIECE_CID_CF),
                    piece_cid.to_bytes(),
                )?;
                continue;
            }

            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L684-L690
            cids.swap_remove(idx);
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L692-L698
            batch.put_cf_cbor(self.cf_handle(MULTIHASH_TO_PIECE_CID_CF), mh_key, cids)?;

            // This might be wrong — might need to reference a CF
            self.database.delete(key)?;
        }

        Ok(self.database.write(batch)?)
    }

    fn next_pieces_to_check(&self, miner_address: String) -> Result<Vec<Cid>, PieceStoreError> {
        // self.database.iterator_cf(self.cf_handle(PIECE_CID_TO_CURSOR_CF), IteratorMode::From((), ()))
        todo!()
    }

    fn flag_piece(
        &self,
        piece_cid: Cid,
        has_unsealed_copy: bool,
        miner_address: String,
    ) -> Result<(), PieceStoreError> {
        let key = format!("{}/{}", piece_cid.to_string(), miner_address);
        let mut metadata = self
            .get_value_at_key(&key, PIECE_CID_TO_FLAGGED_CF)?
            .unwrap_or_else(|| FlaggedMetadata::with_address(miner_address));

        metadata.updated_at = time::OffsetDateTime::now_utc();
        metadata.has_unsealed_copy = has_unsealed_copy;

        self.put_value_at_key(key, &metadata, PIECE_CID_TO_FLAGGED_CF)
    }

    fn unflag_piece(&self, piece_cid: Cid, miner_address: String) -> Result<(), PieceStoreError> {
        let key = format!("{}/{}", piece_cid.to_string(), miner_address);
        self.remove_value_at_key(key, PIECE_CID_TO_FLAGGED_CF)
    }

    fn flagged_pieces_list(
        &self,
        filter: Option<FlaggedPiecesListFilter>,
        cursor: time::OffsetDateTime,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<FlaggedPiece>, PieceStoreError> {
        let iterator = self
            .database
            .iterator_cf(self.cf_handle(PIECE_CID_TO_FLAGGED_CF), IteratorMode::Start);

        let mut flagged_pieces = vec![];
        for line in iterator {
            let (key, value) = line?;

            // This one should never happen but who knows?
            let key = String::from_utf8(key.to_vec())?;
            let mut split = key.split('/');

            // Using let/else instead of .ok_or/.ok_or_else avoids using .clone
            let Some(piece_cid) = split.next() else {
                return Err(PieceStoreError::InvalidFlaggedPieceKeyError(key));
            };
            // They don't actually check that the full key is well formed, they just check if it isn't ill-formed
            // by checking if the length after splitting is != 0 and that the CID is valid
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L740-L748

            let piece_cid = Cid::from_str(piece_cid)?;
            let flagged_metadata = match ciborium::from_reader::<FlaggedMetadata, _>(value.as_ref())
            {
                Ok(value) => Ok(value),
                Err(err) => Err(PieceStoreError::Deserialization(err.to_string())),
            }?;

            if let Some(filter) = &filter {
                // NOTE(@jmg-duarte,05/06/2024): The check order is not arbitrary,
                // it's the same as the order in boostd-data, maybe it has a reason,
                // maybe it doesn't, keeping it the same for now...
                // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L756-L766
                if filter.has_unsealed_copy != flagged_metadata.has_unsealed_copy {
                    continue;
                }

                // NOTE(@jmg-duarte,05/06/2024): We could check the address against the key and
                // possibly avoid deserializing, but the original code only checks after deserializing
                // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L750-L762
                if !filter.miner_address.is_empty()
                    && filter.miner_address != flagged_metadata.miner_address
                {
                    continue;
                }

                if flagged_metadata.created_at < cursor {
                    continue;
                }
            }

            flagged_pieces.push(FlaggedPiece {
                piece_cid,
                miner_address: flagged_metadata.miner_address,
                created_at: flagged_metadata.created_at,
                updated_at: flagged_metadata.updated_at,
                has_unsealed_copy: flagged_metadata.has_unsealed_copy,
            });
        }

        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L776-L778
        flagged_pieces.sort_by(|l, r| l.created_at.cmp(&r.created_at));

        if offset > 0 {
            if offset >= flagged_pieces.len() {
                return Ok(vec![]);
            } else {
                flagged_pieces = flagged_pieces.split_off(offset);
            }
        }

        if flagged_pieces.len() > limit {
            flagged_pieces.truncate(limit);
        }

        Ok(flagged_pieces)
    }

    fn flagged_pieces_count(
        &self,
        filter: Option<FlaggedPiecesListFilter>,
    ) -> Result<u64, PieceStoreError> {
        let iterator = self
            .database
            .iterator_cf(self.cf_handle(PIECE_CID_TO_FLAGGED_CF), IteratorMode::Start);

        if let Some(filter) = filter {
            let mut count: u64 = 0;
            for line in iterator {
                let (_, value) = line?;

                let flagged_metadata =
                    match ciborium::from_reader::<FlaggedMetadata, _>(value.as_ref()) {
                        Ok(value) => Ok(value),
                        Err(err) => Err(PieceStoreError::Deserialization(err.to_string())),
                    }?;

                // NOTE(@jmg-duarte,05/06/2024): The check order is not arbitrary,
                // it's the same as the order in boostd-data, maybe it has a reason,
                // maybe it doesn't, keeping it the same for now...
                // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L823-L829
                if filter.has_unsealed_copy != flagged_metadata.has_unsealed_copy {
                    continue;
                }

                if !filter.miner_address.is_empty()
                    && filter.miner_address != flagged_metadata.miner_address
                {
                    continue;
                }

                count += 1;
            }
            Ok(count)
        } else {
            Ok(iterator.count() as u64)
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, str::FromStr};

    use cid::Cid;
    use rocksdb::DEFAULT_COLUMN_FAMILY_NAME;
    use tempfile::tempdir;

    use super::{RocksDBPieceStore, RocksDBStateStoreConfig};
    use crate::local_index_directory::{
        rdb::{CURSOR_TO_OFFSET_SIZE_CF, PIECE_CID_TO_CURSOR_CF, PIECE_CID_TO_FLAGGED_CF},
        DealInfo, PieceInfo, PieceStoreError, Service,
    };

    fn init_database() -> RocksDBPieceStore {
        let tmp_dir = tempdir().unwrap();
        let config = RocksDBStateStoreConfig {
            path: tmp_dir.path().join("rocksdb"),
        };

        RocksDBPieceStore::new(config).unwrap()
    }

    fn cids_vec() -> Vec<Cid> {
        vec![
            Cid::from_str("QmawceGscqN4o8Y8Fv26UUmB454kn2bnkXV5tEQYc4jBd6").unwrap(),
            Cid::from_str("QmbvrHYWXAU1BuxMPNRtfeF4DS2oPmo5hat7ocqAkNPr74").unwrap(),
            Cid::from_str("QmfRL5b6fPZ851F6E2ZUWX1kC4opXzq9QDhamvU4tJGuyR").unwrap(),
        ]
    }

    fn dummy_deal_info() -> DealInfo {
        DealInfo {
            deal_uuid: uuid::Uuid::new_v4(),
            is_legacy: false,
            chain_deal_id: 1337.into(),
            miner_address: "address".to_string().into(),
            sector_number: 42,
            piece_offset: 10,
            piece_length: 10,
            car_length: 97,
            is_direct_deal: false,
        }
    }

    /// Ensure that the expected column families are initialized.
    #[test]
    fn column_families() {
        let db = init_database();
        assert!(matches!(
            db.get_value_at_key::<_, Vec<u8>>("non_existing_key", DEFAULT_COLUMN_FAMILY_NAME),
            Ok(None)
        ));
        assert!(matches!(
            db.get_value_at_key::<_, Vec<u8>>("non_existing_key", PIECE_CID_TO_FLAGGED_CF),
            Ok(None)
        ));
        assert!(matches!(
            db.get_value_at_key::<_, Vec<u8>>("non_existing_key", PIECE_CID_TO_CURSOR_CF),
            Ok(None)
        ));
        assert!(matches!(
            db.get_value_at_key::<_, Vec<u8>>("non_existing_key", CURSOR_TO_OFFSET_SIZE_CF),
            Ok(None)
        ));
    }

    /// Ensure there's nothing funky going on in the simpler wrappers.
    #[test]
    fn value_at_key() {
        let db = init_database();
        let key = "cids";
        let cids = cids_vec();

        assert!(matches!(
            db.get_value_at_key::<_, Vec<Cid>>(key, DEFAULT_COLUMN_FAMILY_NAME),
            Ok(None)
        ));

        assert!(db
            .put_value_at_key(key, &cids, DEFAULT_COLUMN_FAMILY_NAME)
            .is_ok());

        assert!(matches!(
            db.get_value_at_key::<_, Vec<Cid>>(key, DEFAULT_COLUMN_FAMILY_NAME),
            Ok(Some(_))
        ));

        assert!(db
            .remove_value_at_key(key, DEFAULT_COLUMN_FAMILY_NAME)
            .is_ok());

        assert!(matches!(
            db.get_value_at_key::<_, Vec<Cid>>(key, DEFAULT_COLUMN_FAMILY_NAME),
            Ok(None)
        ));
    }

    #[test]
    fn piece_cid_to_metadata() {
        let db = init_database();
        let cid = Cid::from_str("QmawceGscqN4o8Y8Fv26UUmB454kn2bnkXV5tEQYc4jBd6").unwrap();
        let piece_info = PieceInfo::with_cid(cid);

        assert!(matches!(db.get_piece_cid_to_metadata(cid), Ok(None)));
        assert!(db.set_piece_cid_to_metadata(cid, &piece_info).is_ok());
        let received = db.get_piece_cid_to_metadata(cid);
        assert!(matches!(received, Ok(Some(_))));
        assert_eq!(piece_info, received.unwrap().unwrap());

        assert!(db
            .remove_value_at_key(cid.to_bytes(), PIECE_CID_TO_CURSOR_CF)
            .is_ok());
        assert!(matches!(db.get_piece_cid_to_metadata(cid), Ok(None)));
    }

    // Ensure the cursor logic works.
    #[test]
    fn cursor() {
        let db = init_database();
        assert!(db.get_next_cursor().is_err());
        assert!(db.set_next_cursor(1010).is_ok());
        let cursor = db.get_next_cursor();
        assert_eq!(cursor.unwrap(), (1010, format!("{}/", 1010)));
    }

    /// Ensure `add_deal_for_piece` creates a new [`PieceInfo`] and adds the respective deal
    /// as well as append a second [`DealInfo`].
    #[test]
    fn add_deal_for_piece() {
        let db = init_database();
        let cid = cids_vec()[0];
        // Not real values
        let deal_info = dummy_deal_info();
        let deal_info_2 = DealInfo {
            deal_uuid: uuid::Uuid::new_v4(),
            ..deal_info.clone()
        };

        assert!(matches!(db.get_piece_cid_to_metadata(cid), Ok(None)));
        assert!(db.add_deal_for_piece(cid, deal_info.clone()).is_ok());
        assert!(db.add_deal_for_piece(cid, deal_info_2.clone()).is_ok()); // add a second one

        let piece_info = db.get_piece_cid_to_metadata(cid);
        assert!(matches!(piece_info, Ok(Some(_))));
        assert_eq!(piece_info.unwrap().unwrap().deals[0], deal_info.clone());

        let piece_info = db.get_piece_cid_to_metadata(cid);
        assert!(matches!(piece_info, Ok(Some(_))));
        assert_eq!(piece_info.unwrap().unwrap().deals[1], deal_info_2.clone());
    }

    /// Ensure `add_deal_for_piece` detects duplicates.
    #[test]
    fn duplicate_add_deal_for_piece() {
        let db = init_database();
        let cid = cids_vec()[0];
        // Not real values
        let deal_info = dummy_deal_info();

        assert!(matches!(db.get_piece_cid_to_metadata(cid), Ok(None)));
        assert!(db.add_deal_for_piece(cid, deal_info.clone()).is_ok());
        assert!(db.add_deal_for_piece(cid, deal_info.clone()).is_err());
    }
}
