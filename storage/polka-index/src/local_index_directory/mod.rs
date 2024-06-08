use cid::{multihash, Cid};

use integer_encoding::{VarInt, VarIntReader};
use serde::{Deserialize, Serialize};
use std::string;
use uuid::Uuid;

pub mod ext;
pub mod rdb;

/// Error that can occur when interacting with the [`PieceStore`].
#[derive(Debug, thiserror::Error)]
pub enum PieceStoreError {
    #[error("Deal already exists: {0}")]
    DuplicateDealError(Uuid),

    // TODO(@jmg-duarte,06/06/2024): make this error more specific for improved error messages
    #[error("Not found")]
    NotFoundError,

    #[error("invalid flagged piece key format")]
    InvalidFlaggedPieceKeyError(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error(transparent)]
    RocksDBError(#[from] rocksdb::Error),

    #[error(transparent)]
    MultihashError(#[from] cid::multihash::Error),

    #[error(transparent)]
    FromUtf8Error(#[from] string::FromUtf8Error),

    #[error(transparent)]
    CidError(#[from] cid::Error),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    FromHexError(#[from] hex::FromHexError),
}

pub struct FlaggedPiece {
    pub piece_cid: Cid,
    pub miner_address: String,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    pub has_unsealed_copy: bool,
}

pub struct FlaggedPiecesListFilter {
    pub miner_address: String,
    pub has_unsealed_copy: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlaggedMetadata {
    #[serde(rename = "c")]
    pub created_at: time::OffsetDateTime,

    #[serde(rename = "u")]
    pub updated_at: time::OffsetDateTime,

    #[serde(rename = "huc")]
    pub has_unsealed_copy: bool,

    #[serde(rename = "m")]
    // The miner address is a special type from filecoin-project/go-address
    // however, it's simply a wrapper to string:
    // https://github.com/filecoin-project/go-address/blob/365a7c8d0e85c731c192e65ece5f5b764026e85d/address.go#L39-L40
    pub miner_address: String,
}

impl FlaggedMetadata {
    pub fn with_address(miner_address: String) -> Self {
        let now = time::OffsetDateTime::now_utc();
        Self {
            created_at: now,
            updated_at: now,
            has_unsealed_copy: false,
            miner_address,
        }
    }
}

// https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/model/model.go#L50-L62

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetSize {
    // Offset is the offset into the CAR file of the section, where a section
    // is <section size><cid><block data>
    #[serde(rename = "o")]
    pub offset: u64,

    // Size is the size of the block data (not the whole section)
    #[serde(rename = "s")]
    pub size: u64,
}

impl OffsetSize {
    // NOTE(@jmg-duarte,06/06/2024): I've decided against implementing From/Into as this API is very specific
    // and the traits would be public by default

    /// Convert [`Self`] to bytes, this is done by encoding [`Self::offset`] and [`Self::size`] as [`VarInt`]s
    /// and packing them without padding.
    ///
    /// Source:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L358-L360>
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; self.offset.required_space() + self.size.required_space()];
        let o = self.offset.encode_var(&mut bytes);
        self.size.encode_var(&mut bytes[o..]);
        bytes
    }

    /// Read a byte slice into [`Self`], this is the dual to [`Self::to_bytes`].
    ///
    /// Source:
    /// * <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/db.go#L377-L382>
    pub(crate) fn from_bytes(mut bytes: &[u8]) -> Result<Self, std::io::Error> {
        let offset = bytes.read_varint()?;
        let size = bytes.read_varint()?;
        Ok(Self { offset, size })
    }
}

// Record is the information stored in the index for each block in a piece
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    #[serde(rename = "s")]
    pub cid: Cid,
    pub offset_size: OffsetSize,
}

#[derive(Debug, Clone)]
pub struct CarIndexRecord {
    pub cid: Cid,
    pub offset: u64,
}

impl From<Record> for CarIndexRecord {
    fn from(value: Record) -> Self {
        Self {
            cid: value.cid,
            offset: value.offset_size.offset,
        }
    }
}

/// Metadata about a piece that provider may be storing based on its [`Cid`]. So
/// that, given a [`Cid`] during retrieval, the miner can determine how to
/// unseal it if needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PieceInfo {
    // NOTE(@jmg-duarte,04/06/2024): not sure if this is useful + without it we could implement Default
    pub piece_cid: Cid,

    pub version: String,
    pub indexed_at: time::OffsetDateTime,
    pub complete_index: bool,
    pub deals: Vec<DealInfo>,

    /// Piece cursor for other information, such as offset, etc.
    /// https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L40-L41
    pub cursor: u64,
}

impl PieceInfo {
    pub fn with_cid(piece_cid: Cid) -> Self {
        Self {
            piece_cid,
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L45-L46
            version: "1".to_string(),
            // In Go, time.Time's default is "0001-01-01 00:00:00 +0000 UTC"
            indexed_at: time::OffsetDateTime::UNIX_EPOCH,
            complete_index: false,
            deals: Vec::new(),
            cursor: 0,
        }
    }
}

/// Identifier for a retrieval deal (unique to a client)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DealId(u64);

impl From<u64> for DealId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The miner's address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MinerAddress(String);

impl From<String> for MinerAddress {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Numeric identifier for a sector. It is usually relative to a miner.
type SectorNumber = u64;

/// Information about a single deal for a given piece
///
/// Source:
/// <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/model/model.go#L14-L36>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealInfo {
    // By default, the Eq implementation will use all fields,
    // likewise, it doesn't sound like the best idea since
    // as soon as you change a single detail that isn't the deal UUID
    // what should be a conflicting DealInfo, no longer is.
    // However, in the original implementation they do it like that
    // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L119-L125
    // Note that in Go, there is not operator overloading and
    // == is implicitly defined for all types

    // TODO(@jmg-duarte,05/06/2024): document
    #[serde(rename = "u")]
    pub deal_uuid: uuid::Uuid,
    #[serde(rename = "y")]
    pub is_legacy: bool,

    /// Identifier for a deal.
    ///
    /// See [`DealId`] for more information.
    #[serde(rename = "i")]
    pub chain_deal_id: DealId,

    /// The miner's address.
    ///
    /// See [`MinerAddress`] for more information.-
    #[serde(rename = "m")]
    pub miner_address: MinerAddress,

    // TODO(@jmg-duarte,05/06/2024): convert this into newtype
    #[serde(rename = "s")]
    pub sector_number: SectorNumber,
    #[serde(rename = "o")]
    pub piece_offset: u64,
    #[serde(rename = "l")]
    pub piece_length: u64,
    #[serde(rename = "c")]
    pub car_length: u64,
    #[serde(rename = "d")]
    pub is_direct_deal: bool,
}

// TODO(@jmg-duarte,04/06/2024): document
pub trait Service {
    /// Add [`DealInfo`] pertaining to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist in the index, it will be created before adding the [`DealInfo`].
    /// * If the deal is already present in the piece, returns [`PieceStoreError::DuplicateDealError`].
    fn add_deal_for_piece(
        &self,
        piece_cid: Cid,
        deal_info: DealInfo,
    ) -> Result<(), PieceStoreError>;

    /// Remove a deal with the given [`Uuid`] for the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, `false` will be returned instead of [`PieceStoreError::NotFoundError`].
    fn remove_deal_for_piece(&self, piece_cid: Cid, deal_uuid: Uuid)
        -> Result<(), PieceStoreError>;

    /// Check if the piece with the provided [`Cid`] is indexed.
    ///
    /// * If the piece does not exist, `false` will be returned instead of [`PieceStoreError::NotFoundError`].
    fn is_indexed(&self, piece_cid: Cid) -> Result<bool, PieceStoreError>;

    /// Get when the piece with the provided [`Cid`] was indexed.
    ///
    /// * If the piece does not exist, returns [`PieceStoreError::NotFoundError`].
    fn indexed_at(&self, piece_cid: Cid) -> Result<time::OffsetDateTime, PieceStoreError>;

    /// Check if the piece with the provided [`Cid`] has been fully indexed.
    ///
    /// * If the piece does not exist, returns [`PieceStoreError::NotFoundError`].
    fn is_complete_index(&self, piece_cid: Cid) -> Result<bool, PieceStoreError>;

    /// Get the [`PieceInfo`] pertaining to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`PieceStoreError::NotFoundError`].
    fn get_piece_metadata(&self, piece_cid: Cid) -> Result<PieceInfo, PieceStoreError>;

    /// Remove the [`PieceInfo`] pertaining to the piece with the provided [`Cid`].
    /// It will also remove the piece's indexes.
    ///
    /// * If the piece does not exist, returns [`PieceStoreError::NotFoundError`].
    fn remove_piece_metadata(&self, piece_cid: Cid) -> Result<(), PieceStoreError>;

    /// Get the list of [`DealInfo`] pertaining to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`PieceStoreError::NotFoundError`].
    fn get_piece_deals(&self, piece_cid: Cid) -> Result<Vec<DealInfo>, PieceStoreError>;

    /// List the existing pieces.
    ///
    /// * If no pieces exist, an empty [`Vec`] is returned.
    fn list_pieces(&self) -> Result<Vec<Cid>, PieceStoreError>;

    // The return type in Go is considerably different:
    // type AddIndexProgress struct {
    //     Progress float64 `json:"p"`
    //     Err      string  `json:"e,omitempty"`
    // }
    // But honestly, this makes much more sense (?)
    fn add_index(
        &self,
        piece_cid: Cid,
        records: Vec<Record>,
        is_complete_index: bool,
    ) -> Result<(), PieceStoreError>;

    // Just like `add_index`, the return type here is also slightly different:
    // type IndexRecord struct {
    //     model.Record
    //     PieceStoreError PieceStoreError `json:"e,omitempty"`
    // }
    fn get_index(&self, piece_cid: Cid) -> Result<Vec<Record>, PieceStoreError>;

    fn get_offset_size(
        &self,
        piece_cid: Cid,
        multihash: multihash::Multihash<64>,
    ) -> Result<OffsetSize, PieceStoreError>;

    fn pieces_containing_multihash(
        &self,
        multihash: multihash::Multihash<64>,
    ) -> Result<Vec<Cid>, PieceStoreError>;

    fn remove_indexes(&self, piece_cid: Cid) -> Result<(), PieceStoreError>;
    fn next_pieces_to_check(&self, miner_address: String) -> Result<Vec<Cid>, PieceStoreError>;

    fn flag_piece(
        &self,
        piece_cid: Cid,
        has_unsealed_copy: bool,
        miner_address: String,
    ) -> Result<(), PieceStoreError>;
    fn unflag_piece(&self, piece_cid: Cid, miner_address: String) -> Result<(), PieceStoreError>;
    fn flagged_pieces_list(
        &self,
        filter: Option<FlaggedPiecesListFilter>,
        cursor: time::OffsetDateTime, // this name doesn't make much sense but it's the original one,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<FlaggedPiece>, PieceStoreError>;

    // In Go they use int, which is at least 32bit, but can be more!
    // A count of flagged pieces doesn't seem to ever be negative though (?)
    fn flagged_pieces_count(
        &self,
        filter: Option<FlaggedPiecesListFilter>,
    ) -> Result<u64, PieceStoreError>;
}
