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

// NOTE(@jmg-duarte,04/06/2024): We probably could split the interface according to the respective column family

/// Key for the next free cursor.
///
/// This is not a column family as in the original source code it is not a prefix.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L30-L32>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L54-L56>
pub const NEXT_CURSOR_KEY: &str = "next_cursor";

// # Notes on LevelDB vs RocksDB:
// ## Prefixes & Column Families
// In LevelDB there's no concept of logical partitioning, let alone column families,
// instead and partitioning is achieved by prefixing an identifier to create a namespace.
// However, RocksDB has support for logical partitioning and as such, we take advantage
// of it by mapping the LevelDB prefixes from the original code into proper column families.
//
// ## Transactions
// LevelDB does not support transactions, as such, when using `WriteBatchWithTransaction`
// the TRANSACTION const generic is set to `false`. We may wish to turn it on, but at the
// time of writing (7/6/24) the main focus is on porting, and as such keeping things as
// close as possible to the original implementation.
// Discussion on LevelDB transactions: https://groups.google.com/g/leveldb/c/O_iNRkAoObg

/// Column family name to store the mapping between a [`Cid`] and its cursor.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L34-L37>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L58-L61>
pub const PIECE_CID_TO_CURSOR_CF: &str = "piece_cid_to_cursor";

/// Column family name to store the mapping between [`Multihash`]es and piece [`Cid`]s.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L39-L42>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L62-L64>
pub const MULTIHASH_TO_PIECE_CID_CF: &str = "multihash_to_piece_cids";

/// Column family name to store the flagged piece [`Cid`]s.
///
/// Sources:
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L44-L47>
/// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L66-L68>
pub const PIECE_CID_TO_FLAGGED_CF: &str = "piece_cid_to_flagged";

/// Returns a prefix like `/<cursor>/`
fn key_cursor_prefix(cursor: u64) -> String {
    format!("/{}/", cursor)
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

    /// Construct a new [`Self`] from the provided [`RocksDBStateStoreConfig`].
    ///
    /// * If the database does not exist in the path, it will be created.
    /// * If the column families ([`PIECE_CID_TO_CURSOR_CF`],
    ///   [`MULTIHASH_TO_PIECE_CID_CF`], [`PIECE_CID_TO_FLAGGED_CF`],
    ///   [`CURSOR_TO_OFFSET_SIZE_CF`]) do not exist, they will be created.
    fn new(config: RocksDBStateStoreConfig) -> Result<Self, PieceStoreError>
    where
        Self: Sized,
    {
        let column_families = [
            PIECE_CID_TO_FLAGGED_CF,
            MULTIHASH_TO_PIECE_CID_CF,
            PIECE_CID_TO_CURSOR_CF,
        ]
        .into_iter()
        .map(|cf| ColumnFamilyDescriptor::new(cf, Options::default()));

        let mut opts = Options::default();
        // Creates a new database if it doesn't exist
        opts.create_if_missing(true);
        // Create missing column families
        opts.create_missing_column_families(true);

        Ok(Self {
            database: RocksDB::open_cf_descriptors(&opts, config.path, column_families)?,
        })
    }

    /// Get the column family handle for the given column family name.
    ///
    /// **Invariant**: The column family name MUST exist. *Otherwise this function will panic.*
    ///
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

    /// Get the [`PieceInfo`] for the provided piece [`Cid`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// It is equivalent to boost's `DB.GetPieceCidToMetadata`.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L282-L302>
    fn get_piece_cid_to_metadata(
        &self,
        piece_cid: Cid,
    ) -> Result<Option<PieceInfo>, PieceStoreError> {
        self.get_value_at_key(piece_cid.to_bytes(), PIECE_CID_TO_CURSOR_CF)
    }

    /// Set the [`PieceInfo`] for the provided piece [`Cid`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// It is equivalent to boost's `DB.SetPieceCidToMetadata`.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L267-L280>
    fn set_piece_cid_to_metadata(
        &self,
        piece_cid: Cid,
        metadata: &PieceInfo,
    ) -> Result<(), PieceStoreError> {
        self.put_value_at_key(piece_cid.to_bytes(), metadata, PIECE_CID_TO_CURSOR_CF)
    }

    /// Add mappings from several [`Multihash`]es to a single piece [`Cid`].
    ///
    /// * [`Cid`]s are stored as a [`Vec<Cid>`] — i.e. a single [`Multihash`] can map to multiple [`Cid`]s.
    /// * If the [`Multihash`] already exists in the database, it will append the [`Cid`] to the existing list.
    /// * The [`Cid`] order inside the mapping is *not stable*!
    fn set_multihashes_to_piece_cid<const S: usize>(
        &self,
        record_multihashes: &Vec<Multihash<S>>,
        piece_cid: Cid,
    ) -> Result<(), PieceStoreError> {
        // https://github.com/ipfs/go-datastore/blob/1de47089f5c72b61d91b5cd9043e49fe95771ac0/datastore.go#L97-L106
        let mut batch = WriteBatchWithTransaction::<false>::default();

        for multihash in record_multihashes {
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L166-L167
            let multihash_bytes = multihash.to_bytes();
            // let multihash_key = hex::encode(multihash.to_bytes());
            let mut cids = self
                .get_value_at_key::<_, Vec<Cid>>(&multihash_bytes, MULTIHASH_TO_PIECE_CID_CF)?
                .unwrap_or_default();
            if cids.contains(&piece_cid) {
                continue;
            }
            cids.push(piece_cid);
            batch.put_cf_cbor(
                self.cf_handle(MULTIHASH_TO_PIECE_CID_CF),
                multihash_bytes,
                &cids,
            )?;
        }
        // "commit" the batch, should be equivalent to
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L216-L218
        Ok(self.database.write(batch)?)
    }

    /// Retrieve the list of [`Cid`]s corresponding to a single [`Multihash`].
    fn get_multihash_to_piece_cids<const S: usize>(
        &self,
        multihash: &Multihash<S>,
    ) -> Result<Vec<Cid>, PieceStoreError> {
        let Some(multihash) =
            self.get_value_at_key(multihash.to_bytes(), MULTIHASH_TO_PIECE_CID_CF)?
        else {
            return Err(PieceStoreError::NotFoundError);
        };
        Ok(multihash)
    }

    /// Get the next available cursor.
    ///
    /// Returns [`PieceStoreError::NotFoundError`] if no cursor has been set.
    /// Use [`Self::set_next_cursor`] to set the next cursor.
    ///
    /// The information is stored in the [`rocksdb::DEFAULT_COLUMN_FAMILY_NAME`] column family.
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
    /// The information is stored in the [`rocksdb::DEFAULT_COLUMN_FAMILY_NAME`] column family.
    ///
    /// Source:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L124-L130>
    fn set_next_cursor(&self, cursor: u64) -> Result<(), PieceStoreError> {
        let encoded_cursor = cursor.encode_var_vec();
        Ok(self.database.put(NEXT_CURSOR_KEY, encoded_cursor)?)
    }

    /// Add a [`Record`] to the database under a given cursor prefix.
    fn add_index_record(&self, cursor_prefix: &str, record: Record) -> Result<(), PieceStoreError> {
        let key = format!(
            "{}{}",
            cursor_prefix,
            hex::encode(record.cid.hash().to_bytes())
        );
        Ok(self.database.put(key, record.offset_size.to_bytes())?)
    }

    /// Remove the indexes for a given piece [`Cid`], under the given cursor.
    fn remove_indexes(&self, piece_cid: Cid, cursor: u64) -> Result<(), PieceStoreError> {
        // In the original code they don't add first "/" in the prefix,
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L635-L640
        // but they actually do, if we dig deeper, until the go-ds-leveldb Datastore implementation,
        // the first thing ds.Query does is prepepend the "/" in case it is missing
        // https://github.com/ipfs/go-ds-leveldb/blob/efa3b97d25995dfcd042c476f3e2afe0105d0784/datastore.go#L131-L138

        let cursor_prefix = format!("/{}/", cursor);
        let iterator = self.database.prefix_iterator(&cursor_prefix);
        let mut batch = WriteBatchWithTransaction::<false>::default();

        // NOTE(@jmg-duarte,08/06/2024): the continues are wrong because the batch.Delete will always run
        // as long as it doesnt fail
        for it in iterator {
            let (key, _) = it?;
            // TODO(@jmg-duarte,07/06/2024): add note about the +1 or not
            let (_, mh_key) = key.split_at(cursor_prefix.len());

            // Without the closure, the only alternative is to use goto's to skip from the `return Ok(())` to the deletion of the key
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L655-L702
            (|| {
                let Some(mut cids) =
                    self.get_value_at_key::<_, Vec<Cid>>(mh_key, MULTIHASH_TO_PIECE_CID_CF)?
                else {
                    return Err(PieceStoreError::NotFoundError);
                };

                let Some(idx) = cids.iter().position(|cid| cid == &piece_cid) else {
                    return Ok(());
                };

                // If it is empty or it would become empty, delete the whole entry
                if cids.len() <= 1 {
                    batch.delete_cf(self.cf_handle(MULTIHASH_TO_PIECE_CID_CF), mh_key);
                    return Ok(());
                }

                // Otherwise, just delete from the list and put it back in the DB
                // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L684-L690
                cids.swap_remove(idx);
                // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L692-L698
                batch.put_cf_cbor(self.cf_handle(MULTIHASH_TO_PIECE_CID_CF), mh_key, cids)?;
                Ok(())
            })()?;

            // Cursors are stored in the "default" CF, thus we don't specify a CF
            batch.delete(key);
        }

        Ok(self.database.write(batch)?)
    }
}

impl Service for RocksDBPieceStore {
    /// For a detailed description, see [`Service::add_deal_for_piece`].
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L91-L139>
    fn add_deal_for_piece(
        &self,
        piece_cid: Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError> {
        // Check if the piece exists
        let mut piece_info = self
            .get_piece_cid_to_metadata(piece_cid)?
            .unwrap_or_else(|| PieceInfo::with_cid(piece_cid));

        // Check for the duplicate deal
        if let Some(deal) = piece_info.deals.iter().find(|d| **d == deal_info) {
            return Err(PieceStoreError::DuplicateDealError(deal.deal_uuid));
        }

        // Save the new deal
        piece_info.deals.push(deal_info);
        self.set_piece_cid_to_metadata(piece_cid, &piece_info)
    }

    /// For a detailed description, see [`Service::remove_deal_for_piece`].
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L697-L757>
    fn remove_deal_for_piece(
        &self,
        piece_cid: Cid,
        deal_uuid: Uuid,
    ) -> Result<(), PieceStoreError> {
        let mut piece_info = self.get_piece_metadata(piece_cid)?;

        if let Some(idx) = piece_info
            .deals
            .iter()
            .position(|deal| deal.deal_uuid == deal_uuid)
        {
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L733-L739
            piece_info.deals.swap_remove(idx);
        }

        // If the removed deal was the last one, remove the metadata as well
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L741-L748
        if piece_info.deals.is_empty() {
            return match self.remove_piece_metadata(piece_cid) {
                Ok(()) => Ok(()),
                // First of all, it's kinda weird that the metadata might not be there
                // but in any case, it was going to be deleted, so in this case,
                // not finding it is not an error, just means we don't need to do anything
                Err(PieceStoreError::NotFoundError) => Ok(()),
                Err(err) => Err(err),
            };
        }

        self.put_value_at_key(piece_cid.to_bytes(), &piece_info, PIECE_CID_TO_CURSOR_CF)
    }

    /// For a detailed description, see [`Service::is_indexed`].
    ///
    /// * If the piece does not exist, `false` will be returned instead of [`PieceStoreError::NotFoundError`].
    ///   This is the same behavior the original implementation exhibits[*][1].
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L295-L306>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L444-L469>
    ///
    /// [1]: https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L461-L468
    fn is_indexed(&self, piece_cid: Cid) -> Result<bool, PieceStoreError> {
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            // If the piece does not exist, it's clearly not indexed
            .map_or(false, |piece_info: PieceInfo| {
                // The sentinel value we're using is the Unix epoch, so we check against that
                piece_info.indexed_at != OffsetDateTime::UNIX_EPOCH
            }))
    }

    /// For a detailed description, see [`Service::indexed_at`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L444-L469>
    fn indexed_at(&self, piece_cid: Cid) -> Result<time::OffsetDateTime, PieceStoreError> {
        // The Go implementation seems to return the Unix epoch but returning the error makes more sense
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L461-L468
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            .map(|piece_info: PieceInfo| piece_info.indexed_at)
            .ok_or(PieceStoreError::NotFoundError)?)
    }

    /// For a detailed description, see [`Service::is_complete_index`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L308-L330>
    fn is_complete_index(&self, piece_cid: Cid) -> Result<bool, PieceStoreError> {
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            .map(|piece_info: PieceInfo| piece_info.complete_index)
            .ok_or(PieceStoreError::NotFoundError)?)
    }

    /// For a detailed description, see [`Service::get_piece_metadata`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L173-L196>
    fn get_piece_metadata(&self, piece_cid: Cid) -> Result<PieceInfo, PieceStoreError> {
        self.get_piece_cid_to_metadata(piece_cid)?
            .ok_or(PieceStoreError::NotFoundError)
    }

    // TODO(@jmg-duarte,06/06/2024): double check
    /// For a detailed description, see [`Service::remove_piece_metadata`].
    ///
    /// The information is removed from the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L759-L784>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L591-L623>
    fn remove_piece_metadata(&self, piece_cid: Cid) -> Result<(), PieceStoreError> {
        let piece = self.get_piece_metadata(piece_cid)?;
        // Remove all the multihashes before, as without metadata, they're useless.
        // This operation is made first for consistency — i.e. if this fails
        // For more details, see the original implementation:
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L610-L615
        self.remove_indexes(piece_cid, piece.cursor)?;
        self.remove_value_at_key(piece_cid.to_bytes(), PIECE_CID_TO_CURSOR_CF)
    }

    /// For a detailed description, see [`Service::get_piece_deals`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L198-L224>
    fn get_piece_deals(&self, piece_cid: Cid) -> Result<Vec<DealInfo>, PieceStoreError> {
        Ok(self
            .get_piece_cid_to_metadata(piece_cid)?
            .map(|piece_info: PieceInfo| piece_info.deals)
            .ok_or(PieceStoreError::NotFoundError)?)
    }

    /// For a detailed description, see [`Service::list_pieces`].
    ///
    /// The information is stored in the [`PIECE_CID_TO_CURSOR_CF`] column family.
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L517-L538>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L550-L580>
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

    /// For a detailed description, see [`Service::add_index`].
    ///
    /// The index information is stored in the [`rocksdb::DEFAULT_COLUMN_FAMILY_NAME`] and [`MULTIHASH_TO_PIECE_CID_CF`] column families.
    ///
    /// Note:
    /// * In Boost, this operation is performed by running a goroutine that will feed the returned channel,
    ///   in Rust we there's a mix of things that make life especially difficult for us here, however,
    ///   since the whole [`Service`] relies on the sync API of RocksDB, you should use [`tokio::task::spawn_blocking`].
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L332-L443>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L152-L227>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L351-L363>
    fn add_index(
        &self,
        piece_cid: Cid,
        records: Vec<Record>,
        is_complete_index: bool,
    ) -> Result<(), PieceStoreError> {
        let record_cids = records.iter().map(|r| r.cid.hash().to_owned()).collect();
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L369-L374
        self.set_multihashes_to_piece_cid(&record_cids, piece_cid)?;

        // This looks a bit strange at first but in Go mutability is much more of a thing than in Rust, hence,
        // you get a bunch of parts where a variable is declared (and initialized) to be overwritten in a deeper scope
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L376-L410
        let (mut metadata, cursor_prefix) =
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

        // NOTE(@jmg-duarte,11/06/2024): this could be batched
        records
            .into_iter()
            .map(|record| self.add_index_record(&cursor_prefix, record))
            .collect::<Result<_, _>>()?;

        metadata.indexed_at = time::OffsetDateTime::now_utc();
        self.set_piece_cid_to_metadata(piece_cid, &metadata)
    }

    /// For a detailed description, see [`Service::get_index`].
    ///
    /// The information is stored in the [`rocksdb::DEFAULT_COLUMN_FAMILY_NAME`] column family.
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L253-L294>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L304-L349>
    fn get_index(&self, piece_cid: Cid) -> Result<Vec<Record>, PieceStoreError> {
        let Some(metadata) = self.get_piece_cid_to_metadata(piece_cid)? else {
            return Err(PieceStoreError::NotFoundError);
        };

        // This is equivalent to `db.AllRecords`
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L304-L349
        let cursor_prefix = key_cursor_prefix(metadata.cursor);

        // TODO(@jmg-duarte,06/06/2024): review usage patterns as might be able to place all cursors for this in a single column family
        let iterator = self.database.prefix_iterator(&cursor_prefix);

        let mut records = vec![];
        for it in iterator {
            let (key, value) = it?;
            // With some trickery, we can probably get rid of this allocation
            let key = String::from_utf8(key.to_vec())?
                // The original implementation does `k := r.Key[len(q.Prefix)+1:]`
                // but that is because the underlying query "secretly" prepends a `/`,
                // hence the `+1` in the original implementation, and the lack of one here
                .split_off(cursor_prefix.len());
            let mh_bytes = hex::decode(&key)?;
            let cid = Cid::read_bytes(mh_bytes.as_slice())?;
            let offset_size = OffsetSize::from_bytes(&value)?;
            records.push(Record { cid, offset_size });
        }

        // The main difference here is that we don't need to return IndexRecord since we're not sending
        // the records over a channel, we should be able to just error out as soon as we hit one
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L285-L289

        Ok(records)
    }

    /// For a detailed description, see [`Service::get_offset_size`].
    ///
    /// This information is stored in the [`rocksdb::DEFAULT_COLUMN_FAMILY_NAME`] column family.
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L141-L171>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L365-L383>
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
            // Multihash.String() returns the bytes as hex
            // https://github.com/multiformats/go-multihash/blob/728cc45bec837e8ff5abc3ca3f46bcec52b563d2/multihash.go#L177-L185
            format!(
                "{}{}",
                key_cursor_prefix(cursor),
                hex::encode(multihash.to_bytes())
            ),
            // In the original source, the key is prefixed by the cursor, which is used in other places as well
            rocksdb::DEFAULT_COLUMN_FAMILY_NAME,
        )?
        .ok_or(PieceStoreError::NotFoundError)
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

    /// Sources:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L786-L832>
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L625-L717>
    fn remove_indexes(&self, piece_cid: Cid) -> Result<(), PieceStoreError> {
        let Some(metadata) = self.get_piece_cid_to_metadata(piece_cid)? else {
            return Err(PieceStoreError::NotFoundError);
        };

        // This part is a bit weird, in the original code they don't add first "/" in the prefix
        // which hints that it is in the "global namespace", so here we're searching the default column family
        // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L635-L640
        let cursor_prefix = format!("{}/", metadata.cursor);
        let iterator = self.database.prefix_iterator(&cursor_prefix);
        let mut batch = WriteBatchWithTransaction::<false>::default();

        for it in iterator {
            let (key, _) = it?;
            // TODO(@jmg-duarte,07/06/2024): add note about the +1 or not
            let (_, mh_key) = key.split_at(cursor_prefix.len());
            let Some(mut cids) =
                self.get_value_at_key::<_, Vec<Cid>>(mh_key, MULTIHASH_TO_PIECE_CID_CF)?
            else {
                return Err(PieceStoreError::NotFoundError);
            };

            let Some(idx) = cids.iter().position(|cid| cid == &piece_cid) else {
                continue;
            };

            // If it is empty or it would become empty, delete the whole entry
            if cids.len() <= 1 {
                batch.delete_cf(self.cf_handle(MULTIHASH_TO_PIECE_CID_CF), mh_key);
                continue;
            }

            // Otherwise, just delete from the list and put it back in the DB
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L684-L690
            cids.swap_remove(idx);
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L692-L698
            batch.put_cf_cbor(self.cf_handle(MULTIHASH_TO_PIECE_CID_CF), mh_key, cids)?;

            // Cursors are stored in the "default" CF, thus we don't specify a CF
            batch.delete(key);
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
    use std::str::FromStr;

    use cid::Cid;
    use rocksdb::DEFAULT_COLUMN_FAMILY_NAME;
    use tempfile::tempdir;
    use time::OffsetDateTime;

    use super::{RocksDBPieceStore, RocksDBStateStoreConfig};
    use crate::local_index_directory::{
        rdb::{key_cursor_prefix, PIECE_CID_TO_CURSOR_CF, PIECE_CID_TO_FLAGGED_CF},
        DealInfo, OffsetSize, PieceInfo, PieceStoreError, Record, Service,
    };

    fn init_database() -> RocksDBPieceStore {
        let tmp_dir = tempdir().unwrap();
        let config = RocksDBStateStoreConfig {
            path: tmp_dir.path().join("rocksdb"),
        };
        println!("{:?}", config.path);
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
        assert_eq!(cursor.unwrap(), (1010, key_cursor_prefix(1010)));
    }

    /// Ensure `add_deal_for_piece` creates a new [`PieceInfo`] and adds the respective deal
    /// as well as append a second [`DealInfo`].
    #[test]
    fn add_deal_for_piece() {
        let db = init_database();
        let cid = cids_vec()[0];
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

    #[test]
    fn remove_deal_for_piece() {
        let db = init_database();
        let cid = cids_vec()[0];
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

        assert!(db.remove_deal_for_piece(cid, deal_info_2.deal_uuid).is_ok());
        assert_eq!(db.get_piece_deals(cid).unwrap(), vec![deal_info.clone()]);

        assert!(db.remove_deal_for_piece(cid, deal_info.deal_uuid).is_ok());
        assert!(matches!(
            db.get_piece_deals(cid),
            Err(PieceStoreError::NotFoundError)
        ));
    }

    #[test]
    fn is_indexed() {
        let db = init_database();
        let cid = cids_vec()[0];
        let mut piece_info = PieceInfo::with_cid(cid);

        // PieceInfo hasn't been inserted
        assert_eq!(db.is_indexed(cid).unwrap(), false);
        // Inserted but false
        db.set_piece_cid_to_metadata(cid, &piece_info).unwrap();
        assert_eq!(db.is_indexed(cid).unwrap(), false);
        // Modify and insert
        piece_info.indexed_at = OffsetDateTime::now_utc();
        db.set_piece_cid_to_metadata(cid, &piece_info).unwrap();
        assert!(db.is_indexed(cid).unwrap());
    }

    #[test]
    fn indexed_at() {
        let db = init_database();
        let cid = cids_vec()[0];
        let mut piece_info = PieceInfo::with_cid(cid);
        piece_info.indexed_at = OffsetDateTime::now_utc();

        // Inserted but false
        db.set_piece_cid_to_metadata(cid, &piece_info).unwrap();
        assert!(db.is_indexed(cid).unwrap());
        assert_eq!(db.indexed_at(cid).unwrap(), piece_info.indexed_at);
    }

    #[test]
    fn is_complete_index() {
        let db = init_database();
        let cid = cids_vec()[0];
        let mut piece_info = PieceInfo::with_cid(cid);

        // PieceInfo hasn't been inserted
        assert!(matches!(
            db.is_complete_index(cid),
            Err(PieceStoreError::NotFoundError)
        ));
        // Inserted but false
        db.set_piece_cid_to_metadata(cid, &piece_info).unwrap();
        assert_eq!(db.is_complete_index(cid).unwrap(), false);
        // Modify and insert
        piece_info.complete_index = true;
        db.set_piece_cid_to_metadata(cid, &piece_info).unwrap();
        assert!(db.is_complete_index(cid).unwrap());
    }

    #[test]
    fn list_pieces() {
        let db = init_database();
        let cids = cids_vec();

        assert_eq!(db.list_pieces().unwrap(), vec![]);
        // empty payload since `list_pieces` reads the Cid off of the key
        cids.iter().for_each(|cid| {
            db.put_value_at_key::<_, Vec<u8>>(cid.to_bytes(), &vec![], PIECE_CID_TO_CURSOR_CF)
                .unwrap()
        });
        assert_eq!(db.list_pieces().unwrap(), cids);
    }

    #[test]
    fn get_piece_metadata() {
        let db = init_database();
        let cid = Cid::from_str("QmawceGscqN4o8Y8Fv26UUmB454kn2bnkXV5tEQYc4jBd6").unwrap();
        let piece_info = PieceInfo::with_cid(cid);

        assert!(matches!(
            db.get_piece_metadata(cid),
            Err(PieceStoreError::NotFoundError)
        ));
        assert!(db.set_piece_cid_to_metadata(cid, &piece_info).is_ok());
        let received = db.get_piece_metadata(cid);
        assert!(matches!(received, Ok(_)));
        assert_eq!(piece_info, received.unwrap());
    }

    #[test]
    fn remove_piece_metadata() {
        let db = init_database();
        let cid = Cid::from_str("QmawceGscqN4o8Y8Fv26UUmB454kn2bnkXV5tEQYc4jBd6").unwrap();
        let piece_info = PieceInfo::with_cid(cid);

        assert!(matches!(
            db.get_piece_metadata(cid),
            Err(PieceStoreError::NotFoundError)
        ));
        assert!(db.set_piece_cid_to_metadata(cid, &piece_info).is_ok());
        let received = db.get_piece_metadata(cid);
        assert!(matches!(received, Ok(_)));
        assert_eq!(piece_info, received.unwrap());

        assert!(db.remove_piece_metadata(cid).is_ok());
        // TODO(@jmg-duarte,11/06/2024): add test ensuring that indexes are also removed
        assert!(matches!(
            db.get_piece_metadata(cid),
            Err(PieceStoreError::NotFoundError)
        ));
    }

    #[test]
    fn get_piece_deals() {
        let db = init_database();
        let cid = cids_vec()[0];
        let deal_info = dummy_deal_info();
        let deal_info_2 = DealInfo {
            deal_uuid: uuid::Uuid::new_v4(),
            ..deal_info.clone()
        };

        // Ensure there are no tricks up our sleeves
        assert!(matches!(
            db.get_piece_metadata(cid),
            Err(PieceStoreError::NotFoundError)
        ));
        assert!(matches!(
            db.get_piece_deals(cid),
            Err(PieceStoreError::NotFoundError)
        ));

        assert!(db.add_deal_for_piece(cid, deal_info.clone()).is_ok());
        assert!(db.add_deal_for_piece(cid, deal_info_2.clone()).is_ok()); // add a second one

        assert!(matches!(db.get_piece_deals(cid), Ok(_)));
        assert_eq!(
            db.get_piece_deals(cid).unwrap(),
            vec![deal_info, deal_info_2]
        );
    }

    /// Tests the insertion and retrieval of pieces indexes.
    #[test]
    fn get_index() {
        let db = init_database();
        let cids = cids_vec();
        let cid = cids[0];
        let deal_info = dummy_deal_info();
        let records = vec![
            Record {
                cid: cids[1],
                offset_size: OffsetSize { offset: 0, size: 0 },
            },
            Record {
                cid: cids[2],
                offset_size: OffsetSize {
                    offset: 0,
                    size: 100,
                },
            },
        ];

        // Insert the deal
        assert!(db.add_deal_for_piece(cid, deal_info.clone()).is_ok());
        assert_eq!(db.get_index(cid).unwrap(), vec![]);
        // Add the index records
        db.add_index(cid, records.clone(), false).unwrap();

        // Get the index back
        let received = db.get_index(cid);
        assert!(received.is_ok());
        assert_eq!(received.unwrap(), records);

        // Check the multihash -> cid mapping — i.e. check the add_index side effect
        for record in &records {
            println!("{}", hex::encode(record.cid.hash().to_bytes()));
            let value = db.get_multihash_to_piece_cids(record.cid.hash());
            assert_eq!(value.unwrap(), vec![cid]);
        }
    }
}
