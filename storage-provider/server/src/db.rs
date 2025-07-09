use std::{
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

use metrics::{Counter, Gauge};
use polka_storage_provider_common::sector::UnsealedSector;
use primitives::sector::{SectorNumber, SectorNumberError};
use rocksdb::{
    ColumnFamily, ColumnFamilyDescriptor, Options as DBOptions, TransactionDB, TransactionDBOptions,
};
use serde::{de::DeserializeOwned, Serialize};
use storagext::types::storage_provider::{ConversionError, DealProposal};

#[derive(Debug, thiserror::Error)]
pub enum DBError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    RocksDB(#[from] rocksdb::Error),

    #[error(transparent)]
    Multihash(#[from] cid::multihash::Error),

    #[error(transparent)]
    Conversion(#[from] ConversionError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("unexpected data when trying to serialize given sector type: {0}")]
    InvalidSectorData(serde_json::Error),
}

const ACCEPTED_DEAL_PROPOSALS_CF: &str = "accepted_deal_proposals";
/// Pre-committed and prove-committed sectors
const SECTORS_CF: &str = "sectors";
/// Sector open for adding new pieces.
const UNSEALED_SECTORS_CF: &str = "unsealed_sectors";
/// Sectors that are closed for adding new pieces and are waiting to be sealed.
const PENDING_SEALING_SECTORS_CF: &str = "pending_sealing_sectors";

const COLUMN_FAMILIES: [&str; 4] = [
    ACCEPTED_DEAL_PROPOSALS_CF,
    SECTORS_CF,
    PENDING_SEALING_SECTORS_CF,
    UNSEALED_SECTORS_CF,
];

pub struct DealDB {
    database: TransactionDB,
    last_sector_number: AtomicU32,
}

// Using an empty enum over a unit-struct since the former cannot be built,
// using a module would lead to the same effect but would require a use module::*;
// The key idea here is providing ready-to-use methods to avoid mistakes when using metrics.
enum DbMetrics {}
impl DbMetrics {
    const LAST_SECTOR_NUMBER: &str = "storage_provider.db.last_sector_number";
    const UNSEALED_SECTORS: &str = "storage_provider.db.unsealed_sectors";
    const PENDING_SECTORS: &str = "storage_provider.db.pending_sectors";
    const ACTIVE_SECTORS: &str = "storage_provider.db.active_sectors";

    fn setup() {
        metrics::describe_counter!(
            DbMetrics::LAST_SECTOR_NUMBER,
            "The latest sector number available"
        );
        metrics::describe_gauge!(DbMetrics::UNSEALED_SECTORS, "The number of sealed sectors");
        metrics::describe_gauge!(
            DbMetrics::PENDING_SECTORS,
            "The number of sectors pending sealing"
        );
        metrics::describe_gauge!(
            DbMetrics::ACTIVE_SECTORS,
            "The number of active sectors (pre-committed and being proven)"
        );
    }

    #[inline(always)]
    fn last_sector_number_counter() -> Counter {
        metrics::counter!(Self::LAST_SECTOR_NUMBER)
    }

    #[inline(always)]
    fn active_sectors_gauge() -> Gauge {
        metrics::gauge!(Self::ACTIVE_SECTORS)
    }

    #[inline(always)]
    fn unsealed_sectors_gauge() -> Gauge {
        metrics::gauge!(Self::UNSEALED_SECTORS)
    }

    #[inline(always)]
    fn pending_sectors_gauge() -> Gauge {
        metrics::gauge!(Self::PENDING_SECTORS)
    }
}

impl DealDB {
    pub fn new<P>(path: P) -> Result<Self, DBError>
    where
        P: AsRef<Path>,
    {
        let mut opts = DBOptions::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Lock timeout of 1s by default
        let tx_opts = TransactionDBOptions::default();

        let cfs = COLUMN_FAMILIES
            .into_iter()
            .map(|cf_name| ColumnFamilyDescriptor::new(cf_name, DBOptions::default()));

        let db = Self {
            database: TransactionDB::open_cf_descriptors(&opts, &tx_opts, path, cfs)?,
            last_sector_number: AtomicU32::new(0),
        };
        db.initialize_biggest_sector_number()?;
        db.load_metrics();
        Ok(db)
    }

    fn load_metrics(&self) {
        DbMetrics::setup();
        DbMetrics::last_sector_number_counter()
            .absolute(self.last_sector_number.load(Ordering::Relaxed).into());

        let n_unsealed_sectors = self
            .database
            .full_iterator_cf(
                self.cf_handle(UNSEALED_SECTORS_CF),
                rocksdb::IteratorMode::Start,
            )
            .count() as u32;
        DbMetrics::unsealed_sectors_gauge().set(n_unsealed_sectors);

        let n_pending_sectors = self
            .database
            .full_iterator_cf(
                self.cf_handle(PENDING_SEALING_SECTORS_CF),
                rocksdb::IteratorMode::Start,
            )
            .count() as u32;
        DbMetrics::pending_sectors_gauge().set(n_pending_sectors);

        let n_active_sectors = self
            .database
            .full_iterator_cf(self.cf_handle(SECTORS_CF), rocksdb::IteratorMode::Start)
            .count() as u32;
        DbMetrics::active_sectors_gauge().set(n_active_sectors);
    }

    fn cf_handle(&self, name: &str) -> &ColumnFamily {
        self.database
            .cf_handle(name)
            .expect("column family should have been initialized on database startup")
    }

    /// Takes all the existing sectors sealed and unsealed, finds the maximum sector id.
    /// The simplest way possible of generating an id.
    /// This function is private for a reason. It should only be called once at the DealDB initialization.
    /// And then `last_sector_number` is incremented by `next_sector_number` only
    /// If it was called by multiple threads later than initialization, it could cause a race condition and data erasure.
    fn initialize_biggest_sector_number(&self) -> Result<(), DBError> {
        let mut biggest_sector_number = 0.into();

        let unsealed_sectors = self.database.iterator_cf(
            self.cf_handle(UNSEALED_SECTORS_CF),
            rocksdb::IteratorMode::Start,
        );

        let pending_sealing_sectors = self.database.iterator_cf(
            self.cf_handle(PENDING_SEALING_SECTORS_CF),
            rocksdb::IteratorMode::Start,
        );

        let sealed_sectors = self
            .database
            .iterator_cf(self.cf_handle(SECTORS_CF), rocksdb::IteratorMode::Start);

        // Iterate all sectors and find the biggest sector number
        let all_sectors = unsealed_sectors
            .chain(pending_sealing_sectors)
            .chain(sealed_sectors);
        for item in all_sectors {
            let (key, _) = item?;
            let key: [u8; 4] = key
                .as_ref()
                .try_into()
                .expect("sector's key to be u32 le bytes");
            // Unwrap safe. Can only fail if the sector number was manually
            // inserted in the database.
            let sector_id =
                SectorNumber::new(u32::from_le_bytes(key)).expect("valid sector number");
            biggest_sector_number = std::cmp::max(biggest_sector_number, sector_id);
        }

        // [`Ordering::Relaxed`] can be used here as this function is executed only on start-up and once.
        // We don't mind, it's just a initialization.
        self.last_sector_number
            .store(biggest_sector_number.into(), Ordering::Relaxed);
        Ok(())
    }

    /// Add the proposed (but not signed) deal to the database.
    ///
    /// The deal is first converted to JSON, a CIDv1 of the resulting JSON is built using SHA-256,
    /// the CID is used as key and the deal is stored as JSON. After successfully storing the deal
    /// the CID is returned.
    pub fn add_accepted_proposed_deal(
        &self,
        deal_proposal: &DealProposal,
    ) -> Result<cid::Cid, DBError> {
        let cf_handle = self.cf_handle(ACCEPTED_DEAL_PROPOSALS_CF);

        // We could avoid this allocation by passing the CID as a key
        // but that opens the API to be more error prone :(
        let deal_proposal_cid = deal_proposal.json_cid()?;
        let deal_proposal_key = deal_proposal_cid.to_bytes();
        let deal_proposal_json = serde_json::to_vec(deal_proposal)?;

        // We technically allow duplicate deals to be inserted, however,
        // since they're keyed by their hash, there's no *logical* overwrite
        self.database
            .put_cf(cf_handle, &deal_proposal_key, deal_proposal_json)?;

        Ok(deal_proposal_cid)
    }

    /// Get a proposed (but not signed) deal.
    pub fn get_proposed_deal(
        &self,
        deal_proposal_cid: cid::Cid,
    ) -> Result<Option<DealProposal>, DBError> {
        let Some(deal_proposal_slice) = self.database.get_pinned_cf(
            self.cf_handle(ACCEPTED_DEAL_PROPOSALS_CF),
            deal_proposal_cid.to_bytes(),
        )?
        else {
            return Ok(None);
        };
        let deal_proposal = serde_json::from_reader(deal_proposal_slice.as_ref())
            // SAFETY: this should never fail since the API derives a proper CID from the deal
            // if this happens, it means that someone wrote it from a side channel
            .expect("invalid content was placed in the database from outside this API");
        Ok(deal_proposal)
    }

    /// Remove the proposed (but not signed) deal to the database.
    #[allow(dead_code)] // We're currently not deleting deals, but this may come in handy
    pub fn remove_proposed_deal(&self, deal_proposal_cid: cid::Cid) -> Result<(), DBError> {
        Ok(self.database.delete_cf(
            self.cf_handle(ACCEPTED_DEAL_PROPOSALS_CF),
            deal_proposal_cid.to_bytes(),
        )?)
    }

    pub fn get_sector<SectorType: DeserializeOwned>(
        &self,
        sector_number: SectorNumber,
    ) -> Result<Option<SectorType>, DBError> {
        let Some(sector_slice) = self.database.get_pinned_cf(
            self.cf_handle(SECTORS_CF),
            u32::from(sector_number).to_le_bytes(),
        )?
        else {
            return Ok(None);
        };

        let sector = serde_json::from_reader(sector_slice.as_ref())
            .map_err(|e| DBError::InvalidSectorData(e))?;

        Ok(Some(sector))
    }

    /// Atomically increments sector_id counter, so it can be used as an identifier by a sector.
    /// Prior to all of the calls to this function, `initialize_biggest_sector_id` must be called at the node start-up.
    pub fn next_sector_number(&self) -> Result<SectorNumber, SectorNumberError> {
        // [`Ordering::Relaxed`] can be used here, as it's an update on a single variable.
        // It does not depend on other Atomic variables and it does not matter which thread makes it first.
        // We just need it to be different on every thread that calls it concurrently, so the ids are not duplicated.
        let previous = self.last_sector_number.fetch_add(1, Ordering::Relaxed);
        DbMetrics::last_sector_number_counter().increment(1);
        SectorNumber::try_from(previous + 1)
    }

    fn insert_sector<Sector>(
        &self,
        sector_number: SectorNumber,
        sector: &Sector,
        cf_name: &str,
    ) -> Result<(), DBError>
    where
        Sector: Serialize,
    {
        let cf_handle = self.cf_handle(cf_name);
        let key = u32::from(sector_number).to_le_bytes();
        let json = serde_json::to_vec(&sector)?;
        Ok(self.database.put_cf(cf_handle, key, json)?)
    }

    pub fn save_sector<SectorType: Serialize>(
        &self,
        sector_number: SectorNumber,
        sector: &SectorType,
    ) -> Result<(), DBError> {
        self.insert_sector(sector_number, sector, SECTORS_CF)
    }

    /// Insert an unsealed sector.
    pub fn insert_unsealed_sector(&self, sector: &UnsealedSector) -> Result<(), DBError> {
        self.insert_sector(sector.sector_number, sector, UNSEALED_SECTORS_CF)
            .inspect(|()| DbMetrics::unsealed_sectors_gauge().increment(1))
    }

    /// Insert unsealed sector that is ready for sealing.
    pub fn insert_pending_sealing_sector(&self, sector: &UnsealedSector) -> Result<(), DBError> {
        self.insert_sector(sector.sector_number, sector, PENDING_SEALING_SECTORS_CF)
            .inspect(|()| DbMetrics::pending_sectors_gauge().increment(1))
    }

    /// Removes and returns a given Sector, if it doesn't exist, returns `None`.
    /// Locks the passed key for the duration of this operation!
    ///
    /// Removal is only done on success!
    fn remove_sector<Sector>(
        &self,
        sector_number: SectorNumber,
        cf_name: &str,
    ) -> Result<Option<Sector>, DBError>
    where
        Sector: DeserializeOwned,
    {
        let cf_handle = self.cf_handle(cf_name);
        let sector_number_bytes = u32::from(sector_number).to_le_bytes();

        let txn = self.database.transaction();

        // Effectively *lock* the row
        let unsealed_sector = {
            let Some(slice) = txn.get_pinned_for_update_cf(cf_handle, sector_number_bytes, true)?
            else {
                return Ok(None);
            };
            // This serialization error *should* never happen if you didn't f-up any insert calls
            serde_json::from_reader(slice.as_ref()).map_err(DBError::InvalidSectorData)?
        };

        // Delete the row
        txn.delete_cf(cf_handle, u32::from(sector_number).to_le_bytes())
            .map_err(DBError::from)?;

        txn.commit()?;

        Ok(Some(unsealed_sector))
    }

    /// Removes and returns a given [`UnsealedSector`], if it doesn't exist, returns `None`.
    /// Locks the passed key for the duration of this operation!
    ///
    /// Removal is only done on success!
    pub fn remove_unsealed_sector(
        &self,
        sector_number: SectorNumber,
    ) -> Result<Option<UnsealedSector>, DBError> {
        self.remove_sector(sector_number, UNSEALED_SECTORS_CF)
            .inspect(|_| DbMetrics::unsealed_sectors_gauge().decrement(1))
    }

    /// Removes and returns a given [`UnsealedSector`], if it doesn't exist, returns `None`.
    /// Locks the passed key for the duration of this operation!
    ///
    /// Removal is only done on success!
    pub fn remove_pending_sealing_sector(
        &self,
        sector_number: SectorNumber,
    ) -> Result<Option<UnsealedSector>, DBError> {
        self.remove_sector(sector_number, PENDING_SEALING_SECTORS_CF)
            .inspect(|_| DbMetrics::pending_sectors_gauge().decrement(1))
    }

    /// Iterator over unsealed sectors.
    pub fn iter_unsealed_sectors(
        &self,
    ) -> impl Iterator<Item = Result<UnsealedSector, DBError>> + '_ {
        let cf_handle = self.cf_handle(UNSEALED_SECTORS_CF);
        self.database
            .iterator_cf(cf_handle, rocksdb::IteratorMode::Start)
            .flat_map(|res| {
                res.map(|(_, value)| {
                    // NOTE(@jmg-duarte,03/02/2025): maybe add an error! ?
                    serde_json::from_slice::<UnsealedSector>(&value)
                        .map_err(DBError::InvalidSectorData)
                })
                .map_err(DBError::from)
            })
    }

    // This function is a hack while we don't fully separate the pre-committed sectors from the proven ones
    pub fn measure_active_sectors(&self) {
        let cf_handle = self.cf_handle(SECTORS_CF);
        // SAFETY(cast): total number of sectors is u32
        let active_sectors = self
            .database
            .iterator_cf(cf_handle, rocksdb::IteratorMode::Start)
            .count() as u32;
        DbMetrics::active_sectors_gauge().set(active_sectors);
    }
}
