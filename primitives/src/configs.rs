use frame_support::{
    traits::{Currency, ReservableCurrency},
    Parameter,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::Get;
use sp_runtime::traits::{IdentifyAccount, Verify};

/// Allows to extract Balance of an account via the Config::Currency associated type.
/// BalanceOf is a sophisticated way of getting an u128.
pub type BalanceOf<T> = <<T as CurrencyProvider>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;

pub trait CurrencyProvider: frame_system::Config {
    /// The currency mechanism.
    type Currency: ReservableCurrency<<Self as frame_system::Config>::AccountId>;
}

pub trait MarketProvider: frame_system::Config {
    /// Off-Chain signature type.
    ///
    /// Can verify whether an `Self::OffchainPublic` created a signature.
    type OffchainSignature: Verify<Signer = Self::OffchainPublic> + Parameter;

    /// Off-Chain public key.
    ///
    /// Must identify as an on-chain `Self::AccountId`.
    type OffchainPublic: IdentifyAccount<AccountId = <Self as frame_system::Config>::AccountId>;

    /// How many deals can be published in a single batch of `publish_storage_deals`.
    type MaxDeals: Get<u32>;

    /// How many days should a deal last (activated). Minimum.
    /// Filecoin uses 180 as default.
    /// https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L29
    type MinDealDuration: Get<BlockNumberFor<Self>>;
}

/// Represents functions that are provided by the Storage Provider Pallet
pub trait StorageProviderProvider: frame_system::Config {
    /// Maximum number of blocks past the current block a sector may be set to expire.
    type MaxSectorExpiration: Get<BlockNumberFor<Self>>;

    /// Maximum number of blocks a sector can stay in pre-committed state
    type SectorMaximumLifetime: Get<BlockNumberFor<Self>>;
}
