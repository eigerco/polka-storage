use std::{ops::Deref, string};

use base64::Engine;
use cid::{
    multihash::{self, Multihash},
    Cid,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod rdb;
pub mod rdb_ext;

/// Convert a [`Multihash`] into a key (converts [`Multihash::digest`] to base-64).
///
/// Go encodes []byte as base-64:
/// > Array and slice values encode as JSON arrays,
/// > except that []byte encodes as a base64-encoded string,
/// > and a nil slice encodes as the null JSON value.
/// > — https://pkg.go.dev/encoding/json#Marshal
pub(crate) fn multihash_base64<const S: usize>(multihash: &Multihash<S>) -> String {
    base64::engine::general_purpose::STANDARD.encode(multihash.to_bytes())
}

/// Error that can occur when interacting with the [`Service`].
#[derive(Debug, thiserror::Error)]
pub enum LidError {
    #[error("Deal already exists: {0}")]
    DuplicateDealError(Uuid),

    #[error("Piece {0} was not found")]
    PieceNotFound(Cid),

    #[error("Multihash {:?} was not found", multihash_base64(.0))]
    MultihashNotFound(Multihash<64>),

    #[error("A free cursor was not found")]
    CursorNotFound,

    #[error("Invalid flagged piece key format")]
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
    Base64DecodeError(#[from] base64::DecodeError),
}

/// A [`FlaggedPiece`] is a piece that has been flagged for the user's attention
/// (e.g. the index is missing).
///
/// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/model/model.go#L86-L95>
#[derive(Debug, Serialize, Deserialize)]
pub struct FlaggedPiece {
    pub piece_cid: Cid,
    pub storage_provider_address: StorageProviderAddress,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub has_unsealed_copy: bool,
}

impl FlaggedPiece {
    /// Construct a new [`FlaggedPiece`].
    ///
    /// * `created_at` and `updated_at` will be set to `now`.
    /// * `has_unsealed_copy` will be set to `false`.
    pub fn new(piece_cid: Cid, storage_provider_address: StorageProviderAddress) -> Self {
        let now = chrono::Utc::now();
        Self {
            piece_cid,
            storage_provider_address,
            created_at: now,
            updated_at: now,
            has_unsealed_copy: false,
        }
    }
}

pub struct FlaggedPiecesListFilter {
    pub storage_provider_address: StorageProviderAddress,
    pub has_unsealed_copy: bool,
}

// https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/model/model.go#L50-L62

// NOTE(@jmg-duarte,12/06/2024): `OffsetSize` is currently (de)serialized using CBOR
// however, we can save up on space using the same encoding that the original implementation uses
// which are just two varints, packed together
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffsetSize {
    /// Offset is the offset into the CAR file of the section, where a section
    /// is <section size><cid><block data>
    #[serde(rename = "o")]
    pub offset: u64,

    /// Size is the size of the block data (not the whole section)
    #[serde(rename = "s")]
    pub size: u64,
}

// Record is the information stored in the index for each block in a piece
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRecord {
    /// The [`Cid`] of the indexed block.
    #[serde(rename = "s")]
    pub cid: Cid,

    /// The [`OffsetSize`] for the data — i.e. offset and size.
    pub offset_size: OffsetSize,
}

/// Metadata over a [piece][1], pertaining to the storage of the piece in a given storage provider.
///
/// A piece is the unit of negotiation for data storage.
/// Piece sizes are limited by the sector size, hence,
/// if a user wants to store data larger than the sector size,
/// the data will be split into multiple pieces.
///
/// [1]: https://spec.filecoin.io/systems/filecoin_files/piece/
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PieceInfo {
    /// The piece metadata version, it *will* be used for data migrations.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L25-L27>
    pub version: String,

    /// If present, when the piece was last indexed.
    pub indexed_at: Option<chrono::DateTime<chrono::Utc>>,

    /// If the index has all information or is missing block size information.
    ///
    /// Source: <https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/model/model.go#L42-L46>
    pub complete_index: bool,

    /// Deals that this piece is related to.
    ///
    /// Each deal can only pertain to a single piece, however,
    /// a piece can contain multiple deals — e.g. for redundancy.
    ///
    /// See [`DealInfo`] for more information.
    pub deals: Vec<DealInfo>,

    /// Piece cursor for other information, such as offset, etc.
    /// https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L40-L41
    pub cursor: u64,
}

impl Default for PieceInfo {
    fn default() -> Self {
        Self {
            // https://github.com/filecoin-project/boost/blob/16a4de2af416575f60f88c723d84794f785d2825/extern/boostd-data/ldb/service.go#L45-L46
            version: "1".to_string(),
            // In Go, time.Time's default is "0001-01-01 00:00:00 +0000 UTC"
            // but in Go, structures cannot be `nil`, which is probably why they use that sentinel value
            indexed_at: None,
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

// TODO(@jmg-duarte,14/06/2024): validate miner address

/// The storage provider address.
///
/// It is a special type from `filecoin-project/go-address`
/// however, it's simply a wrapper to `string`:
/// https://github.com/filecoin-project/go-address/blob/365a7c8d0e85c731c192e65ece5f5b764026e85d/address.go#L39-L40
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StorageProviderAddress(String);

// The Deref implementation eases usages like checking whether the address is empty.
impl Deref for StorageProviderAddress {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for StorageProviderAddress {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Numeric identifier for a sector. It is usually relative to a storage provider.
///
/// For more information on sectors, see:
/// <https://spec.filecoin.io/#section-systems.filecoin_mining.sector>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SectorNumber(u64);

impl From<u64> for SectorNumber {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// Information about a single *storage* deal for a given piece.
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
    // Note that in Go, there is no operator overloading and == is implicitly defined for all types
    /// The deal [`Uuid`].
    #[serde(rename = "u")]
    pub deal_uuid: Uuid,

    // NOTE(@jmg-duarte,17/06/2024): this will probably not be needed
    /// Wether this deal was performed using `go-fil-markets`.
    ///
    /// See the following links for more information:
    /// * <https://boost.filecoin.io/configuration/legacy-deal-configuration>
    /// * <https://filecoin.io/blog/posts/make-lightning-fast-storage-deals-with-boost-v1.0>
    #[serde(rename = "y")]
    pub is_legacy: bool,

    /// Identifier for a deal on the chain.
    ///
    /// See [`DealId`] for more information.
    #[serde(rename = "i")]
    pub chain_deal_id: DealId,

    /// The storage provider's address.
    ///
    /// See [`StorageProviderAddress`] for more information.
    #[serde(rename = "m")]
    pub storage_provider_address: StorageProviderAddress,

    /// The sector number where the piece is stored in.
    ///
    /// See [`SectorNumber`] for more information.
    #[serde(rename = "s")]
    pub sector_number: SectorNumber,

    /// The offset of this deal's piece in the [sector][`SectorNumber`].
    #[serde(rename = "o")]
    pub piece_offset: u64,

    /// The length of this deal's piece.
    ///
    /// A full piece will contain a proving tree and a CAR file.
    ///
    /// See:
    /// * <https://spec.filecoin.io/#section-glossary.piece>
    /// * <https://spec.filecoin.io/#section-systems.filecoin_files.piece>
    #[serde(rename = "l")]
    pub piece_length: u64,

    /// The length of the piece's CAR file.
    #[serde(rename = "c")]
    pub car_length: u64,

    /// Wether this deal is a [direct deal][1].
    ///
    /// A direct deal is usually made for data larger than 4Gb as it will contain a single piece,
    /// a non-direct deal is an [aggregated deal][2], which is aggregated from small scale data (< 4Gb).
    ///
    /// [1]: https://docs.filecoin.io/smart-contracts/programmatic-storage/direct-deal-making
    /// [2]: https://docs.filecoin.io/smart-contracts/programmatic-storage/aggregated-deal-making
    #[serde(rename = "d")]
    pub is_direct_deal: bool,
}

pub trait Service {
    /// Add [`DealInfo`] pertaining to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist in the index, it will be created before adding the [`DealInfo`].
    /// * If the deal is already present in the piece, returns [`LidError::DuplicateDealError`].
    fn add_deal_for_piece(&self, piece_cid: Cid, deal_info: DealInfo) -> Result<(), LidError>;

    /// Remove a deal with the given [`Uuid`] for the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, this operation is a no-op.
    fn remove_deal_for_piece(&self, piece_cid: Cid, deal_uuid: Uuid) -> Result<(), LidError>;

    /// Check if the piece with the provided [`Cid`] is indexed.
    ///
    /// * If the piece does not exist, returns `false`.
    fn is_indexed(&self, piece_cid: Cid) -> Result<bool, LidError>;

    /// Get when the piece with the provided [`Cid`] was indexed.
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    fn indexed_at(&self, piece_cid: Cid)
        -> Result<Option<chrono::DateTime<chrono::Utc>>, LidError>;

    /// Check if the index of the piece with the provided [`Cid`] has block size information.
    ///
    /// See [`PieceInfo::complete_index`] for details.
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    fn is_complete_index(&self, piece_cid: Cid) -> Result<bool, LidError>;

    /// Get the [`PieceInfo`] pertaining to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    fn get_piece_metadata(&self, piece_cid: Cid) -> Result<PieceInfo, LidError>;

    /// Remove the [`PieceInfo`] pertaining to the piece with the provided [`Cid`].
    /// It will also remove the piece's indexes.
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    /// * If the piece's indexes are out of sync and its [`Multihash`] entries are not found,
    ///   returns [`LidError::MultihashNotFound`].
    fn remove_piece_metadata(&self, piece_cid: Cid) -> Result<(), LidError>;

    /// Get the list of [`DealInfo`] pertaining to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    fn get_piece_deals(&self, piece_cid: Cid) -> Result<Vec<DealInfo>, LidError>;

    /// List the existing pieces.
    ///
    /// * If no pieces exist, an empty [`Vec`] is returned.
    fn list_pieces(&self) -> Result<Vec<Cid>, LidError>;

    /// Add index records to the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, a new [`PieceInfo`] will be created.
    ///
    /// Differences to the original:
    /// * The original implementation streams the operation progress.
    /// * The original implementation does not support this operation through HTTP.
    fn add_index(
        &self,
        piece_cid: Cid,
        records: Vec<IndexRecord>,
        is_complete_index: bool,
    ) -> Result<(), LidError>;

    /// Get the index records for the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    ///
    /// Differences to the original:
    /// * The original implementation streams the [`OffsetSize`].
    /// * The original implementation does not support this operation through HTTP.
    fn get_index(&self, piece_cid: Cid) -> Result<Vec<IndexRecord>, LidError>;

    /// Get the [`OffsetSize`] of the given [`Multihash`](multihash::Multihash) for the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    /// * If the index entry (i.e. multihash) does not exist, returns [`LidError::MultihashNotFound`].
    fn get_offset_size(
        &self,
        piece_cid: Cid,
        multihash: multihash::Multihash<64>,
    ) -> Result<OffsetSize, LidError>;

    /// Get all the pieces containing the given [`Multihash`](multihash::Multihash).
    ///
    /// * If no pieces are found, returns [`LidError::MultihashNotFound`].
    fn pieces_containing_multihash(
        &self,
        multihash: multihash::Multihash<64>,
    ) -> Result<Vec<Cid>, LidError>;

    /// Remove indexes for the piece with the provided [`Cid`].
    ///
    /// * If the piece does not exist, returns [`LidError::PieceNotFound`].
    /// * If the piece contains index entries — i.e. [`Multihash`] —
    ///   that cannot be found, returns [`LidError::MultihashNotFound`].
    fn remove_indexes(&self, piece_cid: Cid) -> Result<(), LidError>;

    /// Flag the piece with the given [`Cid`].
    ///
    /// * If the piece & storage provider address pair is not found, a new entry will be stored.
    fn flag_piece(
        &self,
        piece_cid: Cid,
        has_unsealed_copy: bool,
        storage_provider_address: StorageProviderAddress,
    ) -> Result<(), LidError>;

    /// Unflag the piece with the given [`Cid`].
    ///
    /// * If the piece & storage provider address pair is not found, this is a no-op.
    fn unflag_piece(
        &self,
        piece_cid: Cid,
        storage_provider_address: StorageProviderAddress,
    ) -> Result<(), LidError>;

    /// List the flagged pieces matching the filter.
    ///
    /// * If the filter is `None`, then all flagged pieces will be matched.
    /// * If no pieces are found, returns an empty [`Vec`].
    /// * Pieces flagged before `cursor` will be filtered out.
    /// * Pieces are sorted according to when they were first flagged — see [`FlaggedPiece::created_at`].
    /// * Offset and limit are applied _after_ sorting the pieces.
    fn flagged_pieces_list(
        &self,
        filter: Option<FlaggedPiecesListFilter>,
        cursor: chrono::DateTime<chrono::Utc>, // this name doesn't make much sense but it's the original one,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<FlaggedPiece>, LidError>;

    /// Count all pieces that match the given filter.
    ///
    /// * If the filter is `None`, then all flagged pieces will be counted.
    /// * If no pieces are found, returns `0`.
    fn flagged_pieces_count(
        &self,
        filter: Option<FlaggedPiecesListFilter>,
    ) -> Result<u64, LidError>;

    /// Returns the [`Cid`]s of the next pieces to be checked for a given storage provider.
    fn next_pieces_to_check(
        &mut self,
        storage_provider_address: StorageProviderAddress,
    ) -> Result<Vec<Cid>, LidError>;
}
