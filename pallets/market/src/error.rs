use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

#[derive(thiserror::Error)]
pub enum DealActivationError {
    /// Deal was tried to be activated by a provider which does not own it
    #[error("DealActivationError: Invalid Provider")]
    InvalidProvider,
    /// Deal should have been activated earlier, it's too late
    #[error("DealActivationError: Start Block Elapsed")]
    StartBlockElapsed,
    /// Sector containing the deal will expire before the deal is supposed to end
    #[error("DealActivationError: Sector Expires Before Deal")]
    SectorExpiresBeforeDeal,
    /// Deal needs to be [`DealState::Published`] if it's to be activated
    #[error("DealActivationError: Invalid Deal State")]
    InvalidDealState,
    /// Tried to activate a deal which is not in the Pending Proposals
    #[error("DealActivationError: Deal Not Pending")]
    DealNotPending,
}

impl core::fmt::Debug for DealActivationError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            DealActivationError::InvalidProvider => {
                write!(f, "DealActivationError: Invalid Provider")
            }
            DealActivationError::StartBlockElapsed => {
                write!(f, "DealActivationError: Start Block Elapsed")
            }
            DealActivationError::SectorExpiresBeforeDeal => {
                write!(f, "DealActivationError: Sector Expires Before Deal")
            }
            DealActivationError::InvalidDealState => {
                write!(f, "DealActivationError: Invalid Deal State")
            }
            DealActivationError::DealNotPending => {
                write!(f, "DealActivationError: Deal Not Pending")
            }
        }
    }
}

/// Errors related to [`DealProposal`] and [`ClientDealProposal`]
/// This is error does not surface externally, only in the logs.
/// Mostly used for Deal Validation [`Self::<T>::validate_deals`].
#[derive(thiserror::Error)]
pub enum ProposalError {
    /// ClientDealProposal.client_signature did not match client's public key and data.
    #[error("ProposalError::WrongClientSignatureOnProposal")]
    WrongClientSignatureOnProposal,
    /// Provider of one of the deals is different than the Provider of the first deal.
    #[error("ProposalError::DifferentProvider")]
    DifferentProvider,
    /// Deal's block_start > block_end, so it doesn't make sense.
    #[error("ProposalError::DealEndBeforeStart")]
    DealEndBeforeStart,
    /// Deal's start block is in the past, it should be in the future.
    #[error("ProposalError::DealStartExpired")]
    DealStartExpired,
    /// Deal has to be [`DealState::Published`] when being Published
    #[error("ProposalError::DealNotPublished")]
    DealNotPublished,
    /// Deal's duration must be within `Config::MinDealDuration` < `Config:MaxDealDuration`.
    #[error("ProposalError::DealDurationOutOfBounds")]
    DealDurationOutOfBounds,
    /// Deal's piece_cid is invalid.
    #[error("ProposalError::InvalidPieceCid")]
    InvalidPieceCid(cid::Error),
    /// Deal's piece_size is invalid.
    #[error("ProposalError::InvalidPieceSize: {0}")]
    InvalidPieceSize(&'static str),
    /// CommD related error
    #[error("ProposalError::CommD: {0}")]
    CommD(&'static str),
}

impl core::fmt::Debug for ProposalError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            ProposalError::WrongClientSignatureOnProposal => {
                write!(f, "ProposalError::WrongClientSignatureOnProposal")
            }
            ProposalError::DifferentProvider => {
                write!(f, "ProposalError::DifferentProvider")
            }
            ProposalError::DealEndBeforeStart => {
                write!(f, "ProposalError::DealEndBeforeStart")
            }
            ProposalError::DealStartExpired => {
                write!(f, "ProposalError::DealStartExpired")
            }
            ProposalError::DealNotPublished => {
                write!(f, "ProposalError::DealNotPublished")
            }
            ProposalError::DealDurationOutOfBounds => {
                write!(f, "ProposalError::DealDurationOutOfBounds")
            }
            ProposalError::InvalidPieceCid(_err) => {
                write!(f, "ProposalError::InvalidPieceCid")
            }
            ProposalError::InvalidPieceSize(err) => {
                write!(f, "ProposalError::InvalidPieceSize: {}", err)
            }
            ProposalError::CommD(err) => {
                write!(f, "ProposalError::CommD: {}", err)
            }
        }
    }
}

impl From<ProposalError> for DispatchError {
    fn from(value: ProposalError) -> Self {
        DispatchError::Other(match value {
            ProposalError::WrongClientSignatureOnProposal => {
                "ProposalError::WrongClientSignatureOnProposal"
            }
            ProposalError::DifferentProvider => "ProposalError::DifferentProvider",
            ProposalError::DealEndBeforeStart => "ProposalError::DealEndBeforeStart",
            ProposalError::DealStartExpired => "ProposalError::DealStartExpired",
            ProposalError::DealNotPublished => "ProposalError::DealNotPublished",
            ProposalError::DealDurationOutOfBounds => "ProposalError::DealDurationOutOfBounds",
            ProposalError::InvalidPieceCid(_err) => "ProposalError::InvalidPieceCid",
            ProposalError::InvalidPieceSize(_err) => "ProposalError::InvalidPieceSize",
            ProposalError::CommD(_err) => "ProposalError::CommD",
        })
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
        match self {
            DealSettlementError::SlashedDeal => {
                write!(f, "DealSettlementError: Slashed Deal")
            }
            DealSettlementError::FutureLastUpdate => {
                write!(f, "DealSettlementError: Future Last Update")
            }
            DealSettlementError::DealNotFound => {
                write!(f, "DealSettlementError: Deal Not Found")
            }
            DealSettlementError::EarlySettlement => {
                write!(f, "DealSettlementError: Early Settlement")
            }
            DealSettlementError::ExpiredDeal => {
                write!(f, "DealSettlementError: Expired Deal")
            }
            DealSettlementError::DealNotActive => {
                write!(f, "DealSettlementError: Deal Not Active")
            }
        }
    }
}

impl From<DealSettlementError> for DispatchError {
    fn from(value: DealSettlementError) -> Self {
        DispatchError::Other(match value {
            DealSettlementError::SlashedDeal => "DealSettlementError: Slashed Deal",
            DealSettlementError::FutureLastUpdate => "DealSettlementError: Future Last Update",
            DealSettlementError::DealNotFound => "DealSettlementError: Deal Not Found",
            DealSettlementError::EarlySettlement => "DealSettlementError: Early Settlement",
            DealSettlementError::ExpiredDeal => "DealSettlementError: Expired Deal",
            DealSettlementError::DealNotActive => "DealSettlementError: Deal Not Active",
        })
    }
}

#[derive(thiserror::Error)]
pub enum SectorTerminateError {
    /// Deal was not found in the [`Proposals`] table.
    #[error("SectorTerminateError: Deal Not Found")]
    DealNotFound,
    /// Caller is not the provider.
    #[error("SectorTerminateError: Invalid Caller")]
    InvalidCaller,
    /// Deal is not active
    #[error("SectorTerminateError: Deal Is Not Active")]
    DealIsNotActive,
}

impl core::fmt::Debug for SectorTerminateError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            SectorTerminateError::DealNotFound => {
                write!(f, "SectorTerminateError: Deal Not Found")
            }
            SectorTerminateError::InvalidCaller => {
                write!(f, "SectorTerminateError: Invalid Caller")
            }
            SectorTerminateError::DealIsNotActive => {
                write!(f, "SectorTerminateError: Deal Is Not Active")
            }
        }
    }
}

impl From<SectorTerminateError> for DispatchError {
    fn from(value: SectorTerminateError) -> Self {
        DispatchError::Other(match value {
            SectorTerminateError::DealNotFound => "SectorTerminateError: Deal Not Found",
            SectorTerminateError::InvalidCaller => "SectorTerminateError: Invalid Caller",
            SectorTerminateError::DealIsNotActive => "SectorTerminateError: Deal Is Not Active",
        })
    }
}
