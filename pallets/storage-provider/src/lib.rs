//! # Storage Provider Pallet
//!
//! This pallet holds information about storage providers and
//! provides an interface to modify information about miners.
//!
//! The Storage Provider Pallet is the source of truth for anything storage provider (the binary) related.
//!
//! At some point this pallet will have to verify proofs submitted by storage providers

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::{Config, Pallet};

use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[derive(Decode, Encode, TypeInfo)]
pub struct MinerInfo<
    AccountId: Encode + Decode + Eq + PartialEq,
    PeerId: Encode + Decode + Eq + PartialEq,
> {
    /// The owner of this miner.
    owner: AccountId,
    /// The miner address
    miner: AccountId,
    /// Miner's libp2p peer id in bytes.
    peer_id: PeerId,
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::MinerInfo;

    use frame_support::dispatch::DispatchResultWithPostInfo;
    use frame_support::ensure;
    use frame_support::pallet_prelude::{IsType, PhantomData, StorageMap};
    use frame_support::traits::{Currency, ReservableCurrency};
    use frame_system::pallet_prelude::OriginFor;
    use frame_system::{ensure_signed, Config as SystemConfig};
    use scale_info::prelude::vec::Vec;

    // Allows to extract Balance of an account via the Config::Currency associated type.
    // BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    /// Peer ID is derived by hashing an encoded public key.
    /// Usually represented in bytes.
    /// https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids
    type PeerId = Vec<u8>;

    #[pallet::pallet]
    #[pallet::without_storage_info] // Allows to define storage items without fixed size
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency mechanism.
        /// Used for rewards, using `ReservableCurrency` over `Currency` because the rewards will be locked
        /// in this pallet until the miner requests the funds through `withdraw_balance`
        type Currency: ReservableCurrency<Self::AccountId>;
    }

    // Need some storage type that keeps track of sectors, deadlines and terminations.
    // Could be added to this type maybe?
    #[pallet::storage]
    #[pallet::getter(fn miners)]
    pub type Miners<T: Config> = StorageMap<_, _, T::AccountId, MinerInfo<T::AccountId, PeerId>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        MinerCreated {
            owner: T::AccountId,
        },
        PeerIdChanged {
            miner: T::AccountId,
            new_peer_id: PeerId,
        },
        OwnerAddressChanged {
            miner: T::AccountId,
            new_owner: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        MinerNotFound,
        InvalidSigner,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Add a new miner information to `Miners`
        #[pallet::call_index(0)]
        pub fn create_miner(
            origin: OriginFor<T>,
            owner: T::AccountId,
            _peer_id: PeerId,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;

            // Generate some miner id and insert into `Miners` storage

            // This probably inherits a `create_miner` function from a `Power` trait.

            Self::deposit_event(Event::MinerCreated { owner });
            todo!()
        }

        /// Update PeerId associated with a given miner.
        #[pallet::call_index(1)]
        pub fn change_peer_id(
            origin: OriginFor<T>,
            miner: T::AccountId,
            peer_id: PeerId,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let who = ensure_signed(origin)?;

            // Get miner info from `Miners` with `who` value
            Miners::<T>::try_mutate(&miner, |maybe_miner| -> DispatchResultWithPostInfo {
                let miner_info = match maybe_miner.as_mut().ok_or(Error::<T>::MinerNotFound) {
                    Ok(info) => info,
                    Err(e) => {
                        log::warn!("Could not get info for miner: {miner:?}");
                        return Err(e.into());
                    }
                };

                // Ensure who is the owner of the miner
                ensure!(who == miner_info.owner, Error::<T>::InvalidSigner);

                log::debug!("Updating peer id for {miner:?}");
                // Update PeerId
                miner_info.peer_id = peer_id.clone();
                Self::deposit_event(Event::PeerIdChanged {
                    miner: miner.clone(),
                    new_peer_id: peer_id,
                });
                Ok(().into())
            })
        }

        // This function updates the owner address to the given `new_owner` for the given `miner`
        #[pallet::call_index(2)]
        pub fn change_owner_address(
            origin: OriginFor<T>,
            miner: T::AccountId,
            new_owner: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;

            // Get miner info from `Miners` with `who` value
            // let miner_info = Miners::<T>::try_get(&miner);

            // Ensure who is the owner of the miner
            // ensure!(who == miner_info.owner)

            // Change owner address

            Self::deposit_event(Event::OwnerAddressChanged { miner, new_owner });
            todo!()
        }

        // Used by the reward pallet to award a block reward to a Miner.
        // I am not sure if this should be implemented on this pallet.
        // The reward pallet could be tightly coupled with the storage provider pallet
        // so the reward pallet can take over this functionality.
        #[pallet::call_index(3)]
        pub fn apply_rewards(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            todo!()
        }

        // This method is used to report a consensus fault by a miner.
        #[pallet::call_index(4)]
        pub fn report_consensus_fault(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            todo!()
        }

        // Used by the Miner's Owner to withdraw available funds earned from block rewards.
        // If the amount to withdraw is larger than what is available the extrinsic will fail.
        #[pallet::call_index(5)]
        pub fn withdraw_balance(
            origin: OriginFor<T>,
            _amount: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            // Check that the extrinsic was signed and get the signer.
            let _who = ensure_signed(origin)?;
            todo!()
        }
    }
}
