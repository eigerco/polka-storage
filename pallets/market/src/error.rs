use codec::{Decode, Encode};
use primitives_commitment::{piece::PaddedPieceSizeError, CommitmentError};
use scale_info::TypeInfo;

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

// TODO: Implement TypeInfo for inner error so we can store them here.
// For now logging will the error will do
#[derive(TypeInfo, Encode, Decode, Clone, PartialEq, thiserror::Error)]
pub enum CommDError {
    #[error("CommDError for commitment {0}")]
    CommitmentError(CommitmentError),
    #[error("CommDError for piece size {0}")]
    PaddedPieceSizeError(PaddedPieceSizeError),
}

impl core::fmt::Debug for CommDError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}
