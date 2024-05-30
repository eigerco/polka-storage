//! # Collator Power Pallet
//!
//! # Overview
//!
//! The Collator Power Pallet provides functions for:
//! - ...

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        pallet_prelude::*,
        sp_runtime::RuntimeDebug,
    };
    use frame_system::{pallet_prelude::*};
    use scale_info::TypeInfo;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Unit of Storage Power of a Miner
        /// E.g. `u128`, used as `number of bytes` for a given SP.
        type StoragePower: Parameter + Member + Clone + MaxEncodedLen;

        /// A stable ID for a Collator
        type CollatorId: Parameter + Member + Ord + MaxEncodedLen;

        /// A stable ID for a Miner
        type MinerId: Parameter + Member + Ord + MaxEncodedLen;
    }

    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct MinerClaim<CollatorId: Ord, StoragePower> {
        /// Number of bytes stored by a Miner
        raw_bytes_power: StoragePower,
        /// Stores how much currency was staked on a particular collator
        staked_power: BoundedBTreeMap<CollatorId, StoragePower, ConstU32<10>>
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn storage_provider_claims)]
    pub type MinerClaims<T: Config> =
        StorageMap<_, _, T::MinerId, MinerClaim<T::CollatorId, T::StoragePower>>;

    #[pallet::event]
    // #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Indicates that a new Miner has been registered.
        /// Newly created Miner does not have any Power.
        /// Power is updated after the Miner proves it has storage available.
        MinerRegistered(T::AccountId),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// If there is an entry in claims map, connected to the AccountId that tries to be registered as a Miner.
        MinerAlreadyRegistered,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// After Miner proved a sector, calls this method to update the bookkeeping about available power.
        pub fn update_storage_power(
            _storage_provider: OriginFor<T>,
            _raw_delta_bytes: T::StoragePower,
        ) -> DispatchResultWithPostInfo {
            todo!()
        }
    }

    /// Functions exposed by the pallet
    /// e.g. `pallet-collator-selection` used them to make decision about the next block producer
    impl<T: Config> Pallet<T> {
    }
}
