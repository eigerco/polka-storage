use frame_support::{pallet_prelude::*, PalletError};
use primitives_commitment::piece::PaddedPieceSizeError as PrimitivesPieceSizeError;
use scale_info::TypeInfo;

#[derive(thiserror::Error, Decode, Encode, PalletError, TypeInfo, RuntimeDebug, PartialEq)]
pub enum PaddedPieceSizeError {
    #[error("minimum piece size is 128 bytes")]
    SizeTooSmall,
    #[error("padded piece size must be a power of 2")]
    SizeNotPowerOfTwo,
    #[error("padded_piece_size is not multiple of NODE_SIZE")]
    NotAMultipleOfNodeSize,
    #[error("piece size is invalid")]
    InvalidPieceCid,
    #[error("unable to create CommD from CID")]
    UnableToCreateCommD,
}

impl From<PrimitivesPieceSizeError> for PaddedPieceSizeError {
    fn from(value: PrimitivesPieceSizeError) -> Self {
        match value {
            PrimitivesPieceSizeError::SizeTooSmall => PaddedPieceSizeError::SizeTooSmall,
            PrimitivesPieceSizeError::SizeNotPowerOfTwo => PaddedPieceSizeError::SizeNotPowerOfTwo,
            PrimitivesPieceSizeError::NotAMultipleOfNodeSize => {
                PaddedPieceSizeError::NotAMultipleOfNodeSize
            }
        }
    }
}

// Clone and PartialEq required because of the BoundedVec<(DealId, DealSettlementError)>
#[derive(TypeInfo, Encode, Decode, Clone, PartialEq, thiserror::Error)]
pub enum DealSettlementError {
    /// The deal is going to be slashed.
    #[error("DealSettlementError: Slashed Deal")]
    SlashedDeal,
    /// The deal last update is in the future — i.e. `last_update_block > current_block`.
    #[error("DealSettlementError: Future Last Update")]
    FutureLastUpdate,
    /// The deal was not found.
    #[error("DealSettlementError: Deal Not Found")]
    DealNotFound,
    /// The deal is too early to settle.
    #[error("DealSettlementError: Early Settlement")]
    EarlySettlement,
    /// The deal has expired
    #[error("DealSettlementError: Expired Deal")]
    ExpiredDeal,
    /// Deal is not activated
    #[error("DealSettlementError: Deal Not Active")]
    DealNotActive,
}

impl core::fmt::Debug for DealSettlementError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}
