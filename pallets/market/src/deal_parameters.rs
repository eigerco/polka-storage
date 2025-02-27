use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

use crate::DealProposal;

/// Bounds for deal duration that storage providers want to accept.
/// Used in the [`OffchainDealParameters`]
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct OffchainDealDurationBound<BlockNumber> {
    pub lower: Option<BlockNumber>,
    pub upper: Option<BlockNumber>,
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
    Balance: PartialOrd + frame_support::traits::tokens::Balance,
    BlockNumber: sp_runtime::traits::BlockNumber,
    // `Balance` and `BlockNumber` are not directly tied to their `Config` counterparts.
    // The structure is flexible enough to be used for other purposes,
    // so we limit the generics to the essential subset of traits only.
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

    /// Validates submitted deal parameters to be within the chains constants
    pub fn validate(
        &self,
        chain_min_duration: BlockNumber,
        chain_max_duration: BlockNumber,
    ) -> bool {
        Balance::zero() < self.minimum_price_per_block
            && self.deal_duration.lower < self.deal_duration.upper
            && (self.deal_duration.lower..=self.deal_duration.upper).contains(&chain_min_duration)
            && (self.deal_duration.lower..=self.deal_duration.upper).contains(&chain_max_duration)
    }
}

/// Converts [`OffchainDealParameters`] to [`DealParameters`]
/// and fills in the `None` with the given chain constants.
/// Conversion function instead of a `From` implementation because
/// we need the chain constants in [`crate::Config`]
pub fn offchain_deal_param_conversion<Balance, BlockNumber>(
    offchain_params: OffchainDealParameters<Balance, BlockNumber>,
    chain_min_duration: BlockNumber,
    chain_max_duration: BlockNumber,
) -> DealParameters<Balance, BlockNumber>
where
    // `BlockNumber` will typically be a number, particularly in our network.
    // As such, the Copy trait should be automatically included and shouldn't require extra work for future implementations.
    // Furthermore, the actual `BlockNumber` trait does require the `Copy` trait.
    // https://docs.rs/sp-runtime/40.1.0/sp_runtime/traits/trait.BlockNumber.html
    BlockNumber: Copy,
{
    let deal_duration = match (
        offchain_params.deal_duration.lower,
        offchain_params.deal_duration.lower,
    ) {
        (None, None) => DealDurationBound {
            lower: chain_min_duration,
            upper: chain_max_duration,
        },
        (Some(lower), None) => DealDurationBound {
            lower,
            upper: chain_max_duration,
        },
        (None, Some(upper)) => DealDurationBound {
            lower: chain_min_duration,
            upper,
        },
        (Some(lower), Some(upper)) => DealDurationBound { lower, upper },
    };
    DealParameters {
        minimum_price_per_block: offchain_params.minimum_price_per_block,
        deal_duration: deal_duration,
    }
}

// An attempt to make a generic DealParameters struct but because it needs to impl TypeInfo to
// Be added in the StorageMap as a value because <T: Config> does not implement TypeInfo
// /// Deal Parameters submitted by the storage provider.
// /// After converting the [`OffchainDealParameters`] Options to chain enforced
// /// values.
// #[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
// pub struct GenericDealParameters<T: Config> {
//     pub minimum_price_per_block: BalanceOf<T>,
//     pub deal_duration: GenericDealDurationBound<T>,
// }

// /// The deal duration bounds submitted by the SP.
// /// Adjusted to have the `None` options in [`OffchainDealDurationBound`]
// /// set to the chain enforced min and max
// #[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
// pub struct GenericDealDurationBound<T: Config> {
//     pub lower: BlockNumberFor<T>,
//     pub upper: BlockNumberFor<T>,
// }

// impl<T> GenericDealParameters<T>
// where
//     T: Config,
// {
//     /// Checks the deal parameters against the given deal parameters
//     /// Returns true if everything checks out
//     /// False if something is out of the duration bound
//     pub fn check_against_proposed_deal<Address>(
//         &self,
//         proposal: &DealProposal<Address, BalanceOf<T>, BlockNumberFor<T>>,
//     ) -> bool {
//         let deal_duration = proposal.end_block - proposal.start_block;
//         proposal.storage_price_per_block >= self.minimum_price_per_block
//             && deal_duration >= self.deal_duration.lower
//             && deal_duration <= self.deal_duration.upper
//     }

//     /// Validates submitted deal parameters to be within the chains constants
//     pub fn validate(&self) -> bool {
//         BalanceOf::<T>::zero() < self.minimum_price_per_block
//             && self.deal_duration.lower < self.deal_duration.upper
//             && (self.deal_duration.lower..=self.deal_duration.upper)
//                 .contains(&T::MinDealDuration::get())
//             && (self.deal_duration.lower..=self.deal_duration.upper)
//                 .contains(&T::MaxDealDuration::get())
//     }
// }

// impl<T> From<OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>>> for GenericDealParameters<T>
// where
//     T: Config,
// {
//     fn from(value: OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>>) -> Self {
//         let deal_duration = match (value.deal_duration.lower, value.deal_duration.lower) {
//             (None, None) => GenericDealDurationBound {
//                 lower: T::MinDealDuration::get(),
//                 upper: T::MaxDealDuration::get(),
//             },
//             (Some(lower), None) => GenericDealDurationBound {
//                 lower,
//                 upper: T::MaxDealDuration::get(),
//             },
//             (None, Some(upper)) => GenericDealDurationBound {
//                 lower: T::MinDealDuration::get(),
//                 upper,
//             },
//             (Some(lower), Some(upper)) => GenericDealDurationBound { lower, upper },
//         };
//         GenericDealParameters {
//             minimum_price_per_block: value.minimum_price_per_block,
//             deal_duration: deal_duration,
//         }
//     }
// }
