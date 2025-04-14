use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use primitives::deals::DealProposal;
use scale_info::TypeInfo;

use crate::error::DealParameterError;

/// Bounds for deal duration that storage providers want to accept.
/// Used in the [`OffchainDealParameters`]
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct OffchainDealDurationBound<BlockNumber> {
    pub lower: Option<BlockNumber>,
    pub upper: Option<BlockNumber>,
}

impl<BlockNumber> OffchainDealDurationBound<BlockNumber>
where
    BlockNumber: PartialOrd + Copy,
{
    /// Validates [`OffchainDealDurationBound`] and places passed in values if any of them are None.
    /// The returns [`DealDurationBound`].
    /// returns Err(()) if something if wrong so we can log it in the pallet.
    fn validate(
        self,
        min_duration: BlockNumber,
        max_duration: BlockNumber,
    ) -> Result<DealDurationBound<BlockNumber>, DealParameterError<BlockNumber>> {
        let lower = self.lower.unwrap_or(min_duration);
        let upper = self.upper.unwrap_or(max_duration);
        if lower > upper {
            return Err(DealParameterError::LowerLargerThanUpper(lower, upper));
        }
        if lower < min_duration {
            return Err(DealParameterError::LowerBoundTooLow(lower, min_duration));
        }
        if upper > max_duration {
            return Err(DealParameterError::UpperBoundTooHigh(upper, max_duration));
        }
        Ok(DealDurationBound { lower, upper })
    }
}

/// The deal duration bounds submitted by the SP.
/// Adjusted to have the `None` options in [`OffchainDealDurationBound`]
/// set to the chain enforced min and max
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct DealDurationBound<BlockNumber> {
    pub lower: BlockNumber,
    pub upper: BlockNumber,
}

/// Deal Parameters submitted by the storage provider.
/// This type gets converted to [`DealParameters`] to set chain
/// defaults for the lower and upper deal bounds.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct OffchainDealParameters<Balance, BlockNumber> {
    pub minimum_price_per_block: Balance,
    pub deal_duration: OffchainDealDurationBound<BlockNumber>,
}

impl<Balance, BlockNumber> OffchainDealParameters<Balance, BlockNumber>
where
    Balance: frame_support::pallet_prelude::Zero,
    BlockNumber: PartialOrd + Copy,
{
    /// Validates [`OffchainDealParameters`] and places passed in values if any of them are None.
    /// The returns [`DealParameters`].
    /// returns Err(String) if something if wrong so we can log it in the pallet.
    pub fn validate(
        self,
        min_duration: BlockNumber,
        max_duration: BlockNumber,
    ) -> Result<DealParameters<Balance, BlockNumber>, DealParameterError<BlockNumber>> {
        if self.minimum_price_per_block.is_zero() {
            return Err(DealParameterError::PriceCannotBeZero);
        }

        Ok(DealParameters {
            minimum_price_per_block: self.minimum_price_per_block,
            deal_duration: self.deal_duration.validate(min_duration, max_duration)?,
        })
    }
}

/// Deal Parameters submitted by the storage provider.
/// After converting the [`OffchainDealParameters`] Options to chain enforced
/// values.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct DealParameters<Balance, BlockNumber> {
    pub minimum_price_per_block: Balance,
    pub deal_duration: DealDurationBound<BlockNumber>,
}

impl<Balance, BlockNumber> DealParameters<Balance, BlockNumber>
where
    Balance: PartialOrd,
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    /// Checks the deal parameters against the given deal parameters
    /// Returns true if everything checks out
    /// False if something is out of the duration bound
    pub fn check_against_proposed_deal<Address>(
        &self,
        proposal: &DealProposal<Address, Balance, BlockNumber>,
    ) -> bool {
        let deal_duration = proposal.end_block - proposal.start_block;
        proposal.storage_price_per_block >= self.minimum_price_per_block
            && deal_duration >= self.deal_duration.lower
            && deal_duration <= self.deal_duration.upper
    }
}
