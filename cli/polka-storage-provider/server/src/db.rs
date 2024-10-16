use std::path::Path;

use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options as DBOptions, DB as RocksDB};
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
}

const ACCEPTED_DEAL_PROPOSALS_CF: &str = "accepted_deal_proposals";

const COLUMN_FAMILIES: [&str; 1] = [ACCEPTED_DEAL_PROPOSALS_CF];

pub struct DealDB {
    database: RocksDB,
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

        Ok(Self {
            database: RocksDB::open_cf_descriptors(&opts, path, cfs)?,
        })
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

    // NOTE(@jmg-duarte,03/10/2024): I think that from here onwards we're very close of reinventing the LID, but so be it
}
