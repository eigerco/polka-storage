use std::{
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

use primitives_proofs::SectorNumber;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options as DBOptions, DB as RocksDB};
use serde::{de::DeserializeOwned, Serialize};
use storagext::types::market::{ConversionError, DealProposal};

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
const SECTORS_CF: &str = "sectors";

const COLUMN_FAMILIES: [&str; 2] = [ACCEPTED_DEAL_PROPOSALS_CF, SECTORS_CF];

pub struct DealDB {
    database: RocksDB,
    last_sector_number: AtomicU32,
}

impl DealDB {
    pub fn new<P>(path: P) -> Result<Self, DBError>
    where
        P: AsRef<Path>,
    {
        let mut opts = DBOptions::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cfs = COLUMN_FAMILIES
            .into_iter()
            .map(|cf_name| ColumnFamilyDescriptor::new(cf_name, DBOptions::default()));

        let db = Self {
            database: RocksDB::open_cf_descriptors(&opts, path, cfs)?,
            last_sector_number: AtomicU32::new(0),
        };

        db.initialize_biggest_sector_number()?;
        Ok(db)
    }

    fn cf_handle(&self, name: &str) -> &ColumnFamily {
        self.database
            .cf_handle(name)
            .expect("column family should have been initialized on database startup")
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
            u64::from(sector_number).to_le_bytes(),
        )?
        else {
            return Ok(None);
        };

        let sector = serde_json::from_reader(sector_slice.as_ref())
            .map_err(|e| DBError::InvalidSectorData(e))?;

        Ok(Some(sector))
    }

    pub fn save_sector<SectorType: Serialize>(
        &self,
        sector_number: SectorNumber,
        sector: &SectorType,
    ) -> Result<(), DBError> {
        let cf_handle = self.cf_handle(SECTORS_CF);
        let key = u64::from(sector_number).to_le_bytes();
        let json = serde_json::to_vec(&sector)?;

        self.database.put_cf(cf_handle, key, json)?;

        Ok(())
    }

    /// Takes all of the existing sectors, finds the maximum sector id.
    /// The simplest way possible of generating an id.
    /// This function is private for a reason. It should only be called once at the DealDB initialization.
    /// And then `last_sector_number` is incremented by `next_sector_number` only
    /// If it was called by multiple threads later than initialization, it could cause a race condition and data erasure.
    fn initialize_biggest_sector_number(&self) -> Result<(), DBError> {
        let mut biggest_sector_number = unsafe { SectorNumber::new_unchecked(0) };
        for item in self
            .database
            .iterator_cf(self.cf_handle(SECTORS_CF), rocksdb::IteratorMode::Start)
        {
            let (key, _) = item?;
            let key: [u8; 4] = key
                .as_ref()
                .try_into()
                .expect("sector's key to be u64 le bytes");
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

    /// Atomically increments sector_id counter, so it can be used as an identifier by a sector.
    /// Prior to all of the calls to this function, `initialize_biggest_sector_id` must be called at the node start-up.
    pub fn next_sector_number(&self) -> Result<SectorNumber, &'static str> {
        // [`Ordering::Relaxed`] can be used here, as it's an update on a single variable.
        // It does not depend on other Atomic variables and it does not matter which thread makes it first.
        // We just need it to be different on every thread that calls it concurrently, so the ids are not duplicated.
        let previous = self.last_sector_number.fetch_add(1, Ordering::Relaxed);
        SectorNumber::try_from(previous + 1)
    }

    // NOTE(@jmg-duarte,03/10/2024): I think that from here onwards we're very close of reinventing the LID, but so be it
}
