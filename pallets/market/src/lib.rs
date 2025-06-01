//! # Market Pallet
//!
//! # Overview
//!
//! Market Pallet provides functions for:
//! - storing balances of Storage Clients and Storage Providers to handle deal collaterals and payouts

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod deal_parameters;
mod dispatchables;
pub mod error;
mod hooks;
pub mod weights;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use cid::Cid;
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        ensure,
        pallet_prelude::*,
        sp_runtime::{
            traits::{AccountIdConversion, CheckedAdd, CheckedSub, Hash, Zero},
            ArithmeticError, RuntimeDebug,
        },
        traits::{ConstU32, Currency, ExistenceRequirement::KeepAlive, Hooks, WithdrawReasons},
        PalletId,
    };
    use frame_system::pallet_prelude::*;
    use primitives::{
        self,
        configs::{BalanceOf, CurrencyProvider, MarketProvider},
        deals::{ClientDealProposal, DealProposal},
        pallets::{ActiveSector, Market, SectorDeal, StorageProviderValidation},
        sector::SectorNumber,
        DealId, MAX_DEALS_PER_SECTOR,
    };
    use scale_info::TypeInfo;
    use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

    use crate::{
        deal_parameters::{DealParameters, OffchainDealParameters},
        error::*,
        weights::WeightInfo,
    };

    pub const LOG_TARGET: &'static str = "runtime::market";

    #[pallet::config]
    pub trait Config: frame_system::Config + CurrencyProvider + MarketProvider {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The pallet weights;
        type WeightInfo: WeightInfo;

        /// PalletId used to derive AccountId which stores funds of the Market Participants.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Storage Provider trait implementation for SP validation to validate that given account id's are registered as SP.
        type StorageProviderValidation: StorageProviderValidation<Self::AccountId>;

        /// How many days should a deal last (activated). Maximum.
        /// Filecoin uses 1278 as default.
        /// https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L29
        #[pallet::constant]
        type MaxDealDuration: Get<BlockNumberFor<Self>>;

        /// How many deals can be scheduled to start at a given block. Maximum.
        /// Those deals are checked by Hook::<T>::on_initialize and it has to have reasonable time complexity.
        /// Having this number too big can affect block production.
        #[pallet::constant]
        type MaxDealsPerBlock: Get<u32>;

        /// How many days should a deal last (activated). Minimum.
        /// Filecoin uses 180 as default.
        /// https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L29
        #[pallet::constant]
        type MinDealDuration: Get<BlockNumberFor<Self>>;
    }

    /// Stores balances info for both Storage Providers and Storage Users
    /// We do not use the ReservableCurrency::reserve mechanism,
    /// as the Market works as a liaison between Storage Providers and Storage Clients.
    /// Market has its own account on which funds of all parties are stored.
    /// It's Market reposibility to manage deposited funds, lock/unlock and pay them out when necessary.
    #[derive(
        Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct BalanceEntry<Balance> {
        /// Amount of Balance that has been deposited for future deals/earned from deals.
        /// It can be withdrawn at any time.
        pub free: Balance,
        /// Amount of Balance that has been staked as Deal Collateral
        /// It's locked to a deal and cannot be withdrawn until the deal ends.
        pub locked: Balance,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub balances: Vec<(T::AccountId, BalanceOf<T>)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                balances: Default::default(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let endowed_accounts = self
                .balances
                .iter()
                .map(|(x, _)| x)
                .cloned()
                .collect::<BTreeSet<_>>();

            assert!(
                endowed_accounts.len() == self.balances.len(),
                "duplicate balances in genesis."
            );

            for &(ref who, free) in self.balances.iter() {
                BalanceTable::<T>::insert(
                    &who,
                    BalanceEntry {
                        free,
                        locked: Zero::zero(),
                    },
                );
            }
        }
    }

    /// [`BalanceTable`] is used to store balances for Storage Market Participants.
    /// Both Clients and Providers track their `free` and `locked` funds.
    /// * `free funds` can be added by `add_balance` method and withdrawn by `withdrawn_balance` method.
    /// * `free funds` are converted to `locked_funds` when staked as collateral for _Deals_.
    /// * `locked funds` cannot be withdrawn freely, first some process need to unlock it.
    /// Invariant must be held at all times:
    /// `account(MarketPallet).balance == all_accounts.map(|balance| balance[account]].locked + balance[account].free).sum()`
    #[pallet::storage]
    pub type BalanceTable<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceEntry<BalanceOf<T>>, ValueQuery>;

    /// Simple incremental ID generator for `Deal` Identification purposes.
    /// Starts as 0, increments once for each published deal.
    /// [`DealId`] is monotonically incremented, does not wrap around.
    /// If there is more [`DealId`]s then u64, panics the runtime (if the chain processed 1M deals / day, it would take ~50539024859 years
    /// to reach the ID limit — for reference Filecoin doesn't even average 200k / day).
    #[pallet::storage]
    pub type NextDealId<T: Config> = StorageValue<_, DealId, ValueQuery>;

    /// Stores all published proposals which are handled by the Market.
    /// Deals are identified by `DealId`.
    /// Proposals are stored here until terminated and settled or expired (not activated in time).
    #[pallet::storage]
    pub type Proposals<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        DealId,
        DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
    >;

    /// Stores Proposals which have been Published but not yet Activated.
    /// Only `T::MaxDeals` Pending Proposals can be held at any time.
    /// `hash_proposal(deal)` is stored in the [`BoundedBTreeSet`].
    /// Stores the Pending Proposals to deduplicate Deals and don't allow to same deal to be Published twice.
    /// Deals could end up having different DealId, but same contents. New deals cannot be deduplicated based on DealId.
    #[pallet::storage]
    pub type PendingProposals<T: Config> =
        StorageValue<_, BoundedBTreeSet<T::Hash, T::MaxDeals>, ValueQuery>;

    /// Stores Published or Activated Deals for each Block.
    /// When Deal is Published it's expected to be activated until a certain Block.
    /// If it's not, Storage Provider is slashed and Client refunded by [`Hooks::on_finalize`].
    /// If it has been activated properly, it's just removed from the map.
    #[pallet::storage]
    pub type DealsForBlock<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedBTreeSet<DealId, T::MaxDealsPerBlock>,
        ValueQuery,
    >;

    /// Holds a mapping from ([`Provider`] [`SectorNumber`]) to its respective [`DealId`]s.
    #[pallet::storage]
    pub type SectorDeals<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (T::AccountId, SectorNumber),
        BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    >;

    /// Holds deal parameters for storage provider
    #[pallet::storage]
    pub type SPDealParameters<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        DealParameters<BalanceOf<T>, BlockNumberFor<T>>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Market Participant deposited free balance to the Market Account.
        BalanceAdded {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Market Participant withdrawn their free balance from the Market Account.
        BalanceWithdrawn {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Deal has been successfully activated.
        DealActivated {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },
        /// Deals were settled.
        DealsSettled {
            /// Deal IDs for those that were successfully settled.
            successful: BoundedVec<SettledDealData<T>, MaxSettleDeals<T>>,
            /// Deal IDs for those that were not successfully settled along with the respective error.
            unsuccessful: BoundedVec<(DealId, DealSettlementError), MaxSettleDeals<T>>,
        },
        /// Deal was slashed.
        /// It means that the `provider_collateral` was burned and the entire client's lockup returned.
        ///
        /// Currently it's emitted only when a deal was supposed to be activated on a given block, but was not.
        /// [`Hooks::on_finalize`] checks deals and slashes them when necessary.
        DealSlashed {
            deal_id: DealId,
            amount: BalanceOf<T>,
            client: T::AccountId,
            provider: T::AccountId,
        },

        /// Deal has been terminated.
        ///
        /// A deal may be voluntarily terminated by the storage provider,
        /// or involuntarily, if the sector has been faulty for 42 consecutive days.
        ///
        /// Source: <https://spec.filecoin.io/#section-systems.filecoin_mining.sector.lifecycle>
        DealTerminated {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },

        /// Batch of published deals.
        DealsPublished {
            provider: T::AccountId,
            deals: BoundedVec<PublishedDeal<T>, T::MaxDeals>,
        },
        /// An SP has updated or published their deal parameters
        DealParametersUpdated {
            provider: T::AccountId,
            deal_parameters: DealParameters<BalanceOf<T>, BlockNumberFor<T>>,
        },
        /// An SP has removed their deal parameters
        DealParametersRemoved { provider: T::AccountId },
    }

    /// Utility type to ensure that the bound for deal settlement is in sync.
    pub type MaxSettleDeals<T> = <T as MarketProvider>::MaxDeals;

    #[derive(TypeInfo, Encode, Decode, Clone, PartialEq)]
    pub struct PublishedDeal<T: Config> {
        pub client: T::AccountId,
        pub deal_id: DealId,
    }

    impl<T> core::fmt::Debug for PublishedDeal<T>
    where
        T: Config,
    {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("PublishedDeal")
                .field("deal_id", &self.deal_id)
                .field("client", &self.client)
                .finish()
        }
    }

    /// The data part of the event pushed when the deal is successfully settled.
    #[derive(TypeInfo, Encode, Decode, Clone, PartialEq)]
    pub struct SettledDealData<T: Config> {
        pub deal_id: DealId,
        pub client: T::AccountId,
        pub provider: T::AccountId,
        pub amount: BalanceOf<T>,
    }

    impl<T> core::fmt::Debug for SettledDealData<T>
    where
        T: Config,
    {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("SettledDealData")
                .field("deal_id", &self.deal_id)
                .field("client", &self.client)
                .field("provider", &self.provider)
                .field("amount", &self.amount)
                .finish()
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Market Participant tries to withdraw more
        /// funds than they have available on the Market, because:
        /// - they never deposited the amount they want to withdraw
        /// - the funds they deposited were locked as part of a deal
        InsufficientFreeFunds,
        /// Tries to unlock funds which have never been locked before.
        InsufficientLockedFunds,
        /// `publish_storage_deals` was called with empty `deals` array.
        NoProposalsToBePublished,
        /// `publish_storage_deals` must be called by Storage Providers and it's a Provider of all of the deals.
        /// This error is emitted when a storage provider tries to publish deals that to not belong to them.
        ProposalsPublishedByIncorrectStorageProvider,
        /// `publish_storage_deals`'s core logic was invoked with a broken invariant that should be called by `validate_deals`.
        UnexpectedValidationError,
        /// There is more than 1 deal of this ID in the Sector.
        DuplicateDeal,
        /// Due to a programmer bug, bounds on Bounded data structures were incorrect so couldn't insert into them.
        DealPreconditionFailed,
        /// Sum of all of the deals piece sizes for a sector exceeds sector size.
        DealsTooLargeToFitIntoSector,
        /// Tried to activate too many deals at a given start_block.
        TooManyDealsPerBlock,
        /// Try to call an operation as a storage provider but the account is not registered as a storage provider.
        StorageProviderNotRegistered,
        /// CommD related error
        CommD,
        /// Tried to propose a deal, but there are too many pending deals.
        /// The pending deals should be activated or wait for expiry.
        TooManyPendingDeals,
        /// Deal was tried to be activated by a provider which does not own it
        InvalidProvider,
        /// Deal should have been activated earlier, it's too late
        StartBlockElapsed,
        /// Sector containing the deal will expire before the deal is supposed to end
        SectorExpiresBeforeDeal,
        /// Deal needs to be [`DealState::Published`] if it's to be activated
        InvalidDealState,
        /// Tried to activate a deal which is not in the Pending Proposals
        DealNotPending,
        /// Deal was not found in the [`Proposals`] table.
        DealNotFound,
        /// Caller is not the provider.
        InvalidCaller,
        /// Deal is not active
        DealIsNotActive,
        /// ClientDealProposal.client_signature did not match client's public key and data.
        WrongClientSignatureOnProposal,
        /// Deal's block_start > block_end, so it doesn't make sense.
        DealEndBeforeStart,
        /// Deal's start block is in the past, it should be in the future.
        DealStartExpired,
        /// Deal has to be [`DealState::Published`] when being Published
        DealNotPublished,
        /// Deal's duration must be within `Config::MinDealDuration` < `Config:MaxDealDuration`.
        DealDurationOutOfBounds,
        /// Deal's piece_cid is invalid.
        InvalidPieceCid,
        /// The proposed deal parameters' falls outside the parameter bounds set by the Storage Provider.
        OutOfBoundsDeal,
        /// The SP attempted to remove DealParameters while there are none present.
        NoDealParamsToRemove,
        /// The deal parameters that the SP submitted are not valid
        InvalidDealParametersSubmitted,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Transfers `amount` of Balance from the `origin` to the Market Pallet account.
        /// It is marked as _free_ in the Market bookkeeping.
        /// Free balance can be withdrawn at any moment from the Market.
        #[pallet::call_index(0)]
        #[pallet::weight((T::WeightInfo::add_balance(), DispatchClass::Normal))]
        pub fn add_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            crate::dispatchables::add_balance::<T>(origin, amount)
        }

        /// Transfers `amount` of Balance from the Market Pallet account to the `origin`.
        /// Only _free_ balance can be withdrawn.
        #[pallet::call_index(1)]
        #[pallet::weight((T::WeightInfo::withdraw_balance(), DispatchClass::Normal))]
        pub fn withdraw_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            crate::dispatchables::withdraw_balance::<T>(origin, amount)
        }

        /// Publish a new set of storage deals (not yet included in a sector).
        /// It saves valid deals as [`DealState::Published`] and locks up client fees and provider's collaterals.
        /// Locked up balances cannot be withdrawn until a deal is terminated.
        /// All of the deals must belong to a single Storage Provider.
        /// It is permissive, if some of the deals are correct and some are not, it emits events for valid deals.
        /// On success emits [`Event::<T>::DealPublished`] for each successful deal.
        #[pallet::call_index(2)]
        #[pallet::weight((T::WeightInfo::publish_storage_deals(deals.len() as u32), DispatchClass::Normal))]
        pub fn publish_storage_deals(
            origin: OriginFor<T>,
            deals: BoundedVec<
                ClientDealProposal<
                    T::AccountId,
                    BalanceOf<T>,
                    BlockNumberFor<T>,
                    T::OffchainSignature,
                >,
                T::MaxDeals,
            >,
        ) -> DispatchResult {
            crate::dispatchables::publish_storage_deals::<T>(origin, deals)
        }

        /// Settle pending deal payments for the given deal IDs.
        ///
        /// This function *should* only fully fail when a block was last updated after its `end_block` target.
        ///
        /// In other cases, the function will return two lists, the successful settlements and the unsuccessful ones.
        ///
        /// A settlement is only fully performed when a deal is active.
        ///
        /// A settlement is unsuccessful when:
        /// * The deal was not found. The returned error is [`DealSettlementError::DealNotFound`].
        /// * The deal's start block is after the current block, meaning it's too early to settle the deal.
        ///   The returned error is [`DealSettlementError::EarlySettlement`].
        /// * The deal has been slashed. The returned error is [`DealSettlementError::SlashedDeal`].
        /// * The deal's last update is after the current block, meaning the deal's last update is in the future.
        ///   The returned error is [`DealSettlementError::FutureLastUpdate`].
        /// * The deal is not active
        #[pallet::call_index(3)]
        #[pallet::weight((T::WeightInfo::settle_deal_payments(deal_ids.len() as u32), DispatchClass::Normal))]
        pub fn settle_deal_payments(
            origin: OriginFor<T>,
            // The original `deals` structure is a bitfield from fvm-ipld-bitfield
            deal_ids: BoundedVec<DealId, MaxSettleDeals<T>>,
        ) -> DispatchResult {
            crate::dispatchables::settle_deal_payments::<T>(origin, deal_ids)
        }

        #[pallet::call_index(4)]
        #[pallet::weight((T::WeightInfo::publish_deal_parameters(2), DispatchClass::Normal))]
        pub fn publish_deal_parameters(
            origin: OriginFor<T>,
            deal_parameters: OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>>,
        ) -> DispatchResult {
            crate::dispatchables::publish_deal_parameters::<T>(origin, deal_parameters)
        }

        #[pallet::call_index(5)]
        #[pallet::weight((T::WeightInfo::remove_deal_parameters(), DispatchClass::Normal))]
        pub fn remove_deal_parameters(origin: OriginFor<T>) -> DispatchResult {
            crate::dispatchables::remove_deal_parameters::<T>(origin)
        }
    }

    /// Functions exposed by the pallet
    impl<T: Config> Pallet<T> {
        /// Retrieve the locked balance for the given account.
        pub fn locked(who: &T::AccountId) -> Option<BalanceOf<T>> {
            // try_get is required because the StorageMap has ValueQuery instead of OptionQuery
            match BalanceTable::<T>::try_get(who) {
                Ok(entry) => Some(entry.locked),
                Err(_) => None,
            }
        }

        /// Retrieve the locked balance for the given account.
        pub fn free(who: &T::AccountId) -> Option<BalanceOf<T>> {
            // try_get is required because the StorageMap has ValueQuery instead of OptionQuery
            match BalanceTable::<T>::try_get(who) {
                Ok(entry) => Some(entry.free),
                Err(_) => None,
            }
        }

        /// Account Id of the Market
        ///
        /// This actually does computation.
        /// If you need to keep using it, make sure you cache it and call it once.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        // Used for deduplication purposes
        // We don't want to store another BTreeSet of DealProposals
        // We only care about hashes.
        // It is not an associated function, because T::Hashing is hard to use inside of there.
        pub fn hash_proposal(
            proposal: &DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
        ) -> T::Hash {
            let bytes = Encode::encode(proposal);
            T::Hashing::hash(&bytes)
        }
    }

    impl<T: Config> Market<T::AccountId, BlockNumberFor<T>, BalanceOf<T>> for Pallet<T> {
        fn lock_pre_commit_funds(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
            // NOTE(@jmg-duarte,21/1/25): unsure if this should emit an event
            lock_funds::<T>(who, amount)
        }

        fn unlock_pre_commit_funds(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
            // NOTE(@aidan,23/1/25): unsure if this should emit an event
            unlock_funds::<T>(who, amount)
        }

        fn slash_pre_commit_funds(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
            // NOTE(@jmg-duarte,21/1/25): unsure if this should emit an event
            slash_and_burn::<T>(who, amount)
        }

        /// Verifies a given set of storage deals is valid for sectors being PreCommitted.
        /// Computes UnsealedCID (CommD) for each sector or None for Committed Capacity sectors.
        /// Currently UnsealedCID is hardcoded as we `compute_commd` remains unimplemented because of #92.
        fn verify_deals_for_activation(
            storage_provider: &T::AccountId,
            sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, ConstU32<MAX_DEALS_PER_SECTOR>>,
        ) -> Result<BoundedVec<Option<Cid>, ConstU32<MAX_DEALS_PER_SECTOR>>, DispatchError>
        {
            crate::dispatchables::verify_deals_for_activation::<T>(storage_provider, sector_deals)
        }

        /// Activate a set of deals grouped by sector, returning the size and
        /// extra info about verified deals.
        /// Sectors' deals are activated in parameter-defined order.
        /// Each sector's deals are activated or fail as a group, but independently of other sectors.
        /// Note that confirming all deals fit within a sector is the caller's responsibility
        /// (and is implied by confirming the sector's data commitment is derived from the deal pieces).
        /// PRE-COND: The caller of this function needs to make sure that the `storage_provider` account that is passed in is a registered storage provider.
        fn activate_deals(
            storage_provider: &T::AccountId,
            sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, ConstU32<MAX_DEALS_PER_SECTOR>>,
            compute_cid: bool,
        ) -> Result<
            BoundedVec<ActiveSector<T::AccountId>, ConstU32<MAX_DEALS_PER_SECTOR>>,
            DispatchError,
        > {
            crate::dispatchables::activate_deals::<T>(storage_provider, sector_deals, compute_cid)
        }

        /// Terminate a set of deals in response to their sector being terminated.
        ///
        /// Slashes the provider collateral, refunds the partial unpaid escrow amount to the client.
        ///
        /// A sector can be terminated voluntarily — the storage provider terminates the sector —
        /// or involuntarily — the sector has been faulty for more than 42 consecutive days.
        ///
        /// Source: <https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L786-L876>
        fn on_sectors_terminate(
            storage_provider: &T::AccountId,
            sectors: BoundedVec<SectorNumber, ConstU32<MAX_DEALS_PER_SECTOR>>,
        ) -> DispatchResult {
            crate::dispatchables::on_sectors_terminate::<T>(storage_provider, sectors)
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            // TODO(@th7nder,#77,26/06/2024): set proper weights according to what does the `on_finalize` do
            // return placeholder for now
            // the correct way: get number of deals for a given block from DealsForBlock
            // and then calculate weights according to the actions performed in on_finalize
            T::DbWeight::get().reads(1)
        }

        /// When deals are published in [`publish_storage_deals`], they're added to the `DealsForBlock::<T>::get(current_block)` data structure.
        /// When they are activated in [`activate_deal`], their state is changed from `DealState::Published` to `DealState::Active`
        /// If it did not happen, when [`on_finalize`] reaches `current_block`, it gets Deals that were supposed to be `DealState::Active` from `DealForBlock`.
        /// If they are not `DealState::Active`, hook slashes the Storage Provider and returns all of the funds to the Client.
        ///
        /// *This function should not fail at any point, if it fails, it's a bug.*
        fn on_finalize(current_block: BlockNumberFor<T>) {
            crate::hooks::on_finalize::<T>(current_block)
        }
    }

    /// Unlock a given `amount` of funds from the target account.
    ///
    /// Moves funds from `locked` to `free`.
    #[inline(always)]
    pub fn unlock_funds<T: Config>(
        account_id: &T::AccountId,
        amount: BalanceOf<T>,
    ) -> DispatchResult {
        BalanceTable::<T>::try_mutate(account_id, |balance| -> DispatchResult {
            ensure!(
                balance.locked >= amount,
                Error::<T>::InsufficientLockedFunds
            );
            balance.locked = balance
                .locked
                .checked_sub(&amount)
                .ok_or(ArithmeticError::Underflow)?;

            balance.free = balance
                .free
                .checked_add(&amount)
                .ok_or(ArithmeticError::Overflow)?;

            Ok(())
        })
    }

    /// Lock a given `amount` of funds from the target account.
    ///
    /// Moves funds from `free` to `locked`.
    #[inline(always)]
    pub fn lock_funds<T: Config>(
        account_id: &T::AccountId,
        amount: BalanceOf<T>,
    ) -> DispatchResult {
        BalanceTable::<T>::try_mutate(account_id, |balance| -> DispatchResult {
            ensure!(balance.free >= amount, {
                log::error!(target: LOG_TARGET, "lock_funds: not enough free balance {:?} < {:?}", balance.free, amount);
                Error::<T>::InsufficientFreeFunds
            });

            balance.free = balance
                .free
                .checked_sub(&amount)
                .ok_or(ArithmeticError::Underflow)?;

            balance.locked = balance
                .locked
                .checked_add(&amount)
                .ok_or(ArithmeticError::Overflow)?;

            Ok(())
        })
    }

    /// Slash and burn the provided `amount` from a given account.
    ///
    /// Sets `locked` to `locked - amount` and burns `amount`.
    pub fn slash_and_burn<T: Config>(
        account_id: &T::AccountId,
        amount: BalanceOf<T>,
    ) -> DispatchResult {
        BalanceTable::<T>::try_mutate(account_id, |balance| -> DispatchResult {
            ensure!(
                balance.locked >= amount,
                Error::<T>::InsufficientLockedFunds
            );
            balance.locked = balance
                .locked
                .checked_sub(&amount)
                .ok_or(ArithmeticError::Underflow)?;
            Ok(())
        })?;
        // Burn from circulating supply
        let imbalance = T::Currency::burn(amount);
        // Remove burned amount from the market account
        T::Currency::settle(
            &T::PalletId::get().into_account_truncating(),
            imbalance,
            WithdrawReasons::FEE,
            KeepAlive,
        )
        // If we burned X, tried to settle X and failed, we're in a bad state
        .map_err(|_| DispatchError::Corruption)
    }
}
