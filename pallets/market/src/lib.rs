//! # Market Pallet
//!
//! # Overview
//!
//! Market Pallet provides functions for:
//! - storing balances of Storage Clients and Storage Providers to handle deal collaterals and payouts

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

// TODO(@th7nder,#77,14/06/2024): take the pallet out of dev mode
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    pub const CID_CODEC: u64 = 0x55;
    pub const LOG_TARGET: &'static str = "runtime::market";

    use cid::Cid;
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult,
        ensure,
        pallet_prelude::*,
        sp_runtime::{
            traits::{AccountIdConversion, CheckedAdd, CheckedSub, Hash, IdentifyAccount, Verify},
            ArithmeticError, BoundedBTreeMap, RuntimeDebug,
        },
        traits::{
            Currency,
            ExistenceRequirement::{AllowDeath, KeepAlive},
            Hooks, ReservableCurrency, WithdrawReasons,
        },
        PalletId,
    };
    use frame_system::{pallet_prelude::*, Config as SystemConfig, Pallet as System};
    use multihash_codetable::{Code, MultihashDigest};
    use primitives_proofs::{
        ActiveDeal, ActiveSector, DealId, Market, RegisteredSealProof, SectorDeal, SectorNumber,
        SectorSize, MAX_DEALS_FOR_ALL_SECTORS, MAX_DEALS_PER_SECTOR, MAX_SECTORS_PER_CALL,
    };
    use scale_info::TypeInfo;
    use sp_arithmetic::traits::BaseArithmetic;
    use sp_std::vec::Vec;

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency mechanism.
        type Currency: ReservableCurrency<Self::AccountId>;

        /// PalletId used to derive AccountId which stores funds of the Market Participants.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Off-Chain signature type.
        ///
        /// Can verify whether an `Self::OffchainPublic` created a signature.
        type OffchainSignature: Verify<Signer = Self::OffchainPublic> + Parameter;

        /// Off-Chain public key.
        ///
        /// Must identify as an on-chain `Self::AccountId`.
        type OffchainPublic: IdentifyAccount<AccountId = Self::AccountId>;

        /// How many deals can be published in a single batch of `publish_storage_deals`.
        #[pallet::constant]
        type MaxDeals: Get<u32>;

        /// How many blocks are created in a day (time unit used for calculation)
        #[pallet::constant]
        type BlocksPerDay: Get<BlockNumberFor<Self>>;

        /// How many days should a deal last (activated). Minimum.
        /// Filecoin uses 180 as default.
        /// https://github.com/filecoin-project/builtin-actors/blob/c32c97229931636e3097d92cf4c43ac36a7b4b47/actors/market/src/policy.rs#L29
        #[pallet::constant]
        type MinDealDuration: Get<BlockNumberFor<Self>>;

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
        pub(crate) free: Balance,
        /// Amount of Balance that has been staked as Deal Collateral
        /// It's locked to a deal and cannot be withdrawn until the deal ends.
        pub(crate) locked: Balance,
    }

    #[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub enum DealState<BlockNumber> {
        /// Deal has been accepted on-chain by both Storage Provider and Storage Client, it's waiting for activation.
        Published,
        /// Deal has been activated
        Active(ActiveDealState<BlockNumber>),
    }

    #[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    /// State only related to the activated deal
    /// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/deal.rs#L138>
    pub struct ActiveDealState<BlockNumber> {
        /// Sector in which given piece has been included
        pub(crate) sector_number: SectorNumber,

        /// At which block (time) the deal's sector has been activated.
        pub(crate) sector_start_block: BlockNumber,

        /// The last block (time) when the deal was updated — i.e. when a deal payment settlement was made.
        ///
        /// In Filecoin this happens under two circumstances:
        /// * Someone starts the payment settlement procedure.
        /// * Cron tick (deprecated) settles legacy deals.
        ///
        /// Sources:
        /// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L985>
        /// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1315>
        pub(crate) last_updated_block: Option<BlockNumber>,

        /// When the deal was last slashed, can be never.
        ///
        /// In Filecoin, slashing can happen in two cases, storage faults and consensus faults,
        /// in our case, we're only concerned about the storage faults, as the consensus is
        /// handled by the collators.
        ///
        /// Slashing is related to three main kinds of penalties:
        /// * Fault Fee — incurred for each day a sector is offline.
        /// * Storage Penalty — incurred when sectors that were not declared as faulty before a WindowPoSt are detected.
        /// * Termination Penalty — incurred when a sector is voluntarily (the miner "gave up on the deal") or
        ///   involuntarily (when a sector is faulty for 42 days in a row) terminated and removed from the network.
        ///
        /// Slashing is applied (i.e. `slash_epoch` is updated) in a single place:
        /// * During [`on_miners_sector_terminate`][1], by termination penalty since the deal was terminated early.
        ///   The deal is first settled — i.e. the storage provider gets paid for the storage time since they last settled the deal —
        ///   then storage provider has their collateral slashed and burned and the client gets their funds unlocked (i.e. refunded).
        ///
        /// However, slashing is performed in other places, it just does not update `slash_epoch` (`slash_block` in our case).
        /// * During [`get_active_deal_or_process_timeout`][2], slashing will happen if the deal has expired
        ///   — i.e. if and when the deal is published but fails to be activated in a given period.
        ///   This function is called in [`cron_tick`][3] and [`settle_deal_payments`][4].
        /// * During [`process_deal_update`][5], if the deal has a `slash_epoch`, any remaining payments will be settled
        ///   and the provider will have its collateral slashed.
        /// * During [`cron_tick`][7], by means of [`get_active_deal_or_process_timeout`][8] and finally [`process_deal_init_timed_out`][9].
        ///
        /// Sources:
        /// * <https://spec.filecoin.io/#section-glossary.storage-fault-slashing>
        /// * <https://spec.filecoin.io/#section-systems.filecoin_mining.sector.lifecycle>
        ///
        /// [1]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L852-L853
        /// [2]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L741-L797
        /// [3]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L904-L924
        /// [4]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1240-L1271
        /// [5]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L886-L912
        /// [6]: https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/state.rs#L922-L962
        /// [7]: https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L904-L924
        /// [8]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L765
        /// [9]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L964-L997
        pub(crate) slash_block: Option<BlockNumber>,
    }

    impl<BlockNumber> ActiveDealState<BlockNumber> {
        fn new(
            sector_number: SectorNumber,
            sector_start_block: BlockNumber,
        ) -> ActiveDealState<BlockNumber> {
            ActiveDealState {
                sector_number,
                sector_start_block,
                last_updated_block: None,
                slash_block: None,
            }
        }
    }

    #[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    /// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/deal.rs#L93>
    // It cannot be generic over <T: Config> because, #[derive(RuntimeDebug, TypeInfo)] also make `T` to have `RuntimeDebug`/`TypeInfo`
    // It is a known rust issue <https://substrate.stackexchange.com/questions/452/t-doesnt-implement-stdfmtdebug>
    pub struct DealProposal<Address, Balance, BlockNumber> {
        /// Byte Encoded Cid
        // We use BoundedVec here, as cid::Cid do not implement `TypeInfo`, so it cannot be saved into the Runtime Storage.
        // It maybe doable using newtype pattern, however not sure how the UI on the frontend side would handle that anyways.
        // There is Encode/Decode implementation though, through the feature flag: `scale-codec`.
        pub piece_cid: BoundedVec<u8, ConstU32<128>>,
        pub piece_size: u64,
        /// Storage Client's Account Id
        pub client: Address,
        /// Storage Provider's Account Id
        pub provider: Address,

        /// Arbitrary client chosen label to apply to the deal
        pub label: BoundedVec<u8, ConstU32<128>>,

        /// Nominal start block. Deal payment is linear between StartBlock and EndBlock,
        /// with total amount StoragePricePerBlock * (EndBlock - StartBlock).
        /// Storage deal must appear in a sealed (proven) sector no later than StartBlock,
        /// otherwise it is invalid.
        pub start_block: BlockNumber,
        /// When the Deal is supposed to end.
        pub end_block: BlockNumber,
        /// `Deal` can be terminated early, by `on_sectors_terminate`.
        /// Before that, a Storage Provider can payout it's earned fees by calling `on_settle_deal_payments`.
        /// `on_settle_deal_payments` must know how much money it can payout, so it's related to the number of blocks (time) it was stored.
        /// Reference <https://spec.filecoin.io/#section-systems.filecoin_markets.onchain_storage_market.storage_deal_states>
        pub storage_price_per_block: Balance,

        /// Amount of Balance (DOTs) Storage Provider stakes as Collateral for storing given `piece_cid`
        /// There should be enough Balance added by `add_balance` by Storage Provider to cover it.
        /// When the Deal fails/is terminated to early, this is the amount which get slashed.
        pub provider_collateral: Balance,
        /// Current [`DealState`].
        /// It goes: `Published` -> `Active`
        pub state: DealState<BlockNumber>,
    }

    impl<Address, Balance: BaseArithmetic + Copy, BlockNumber: BaseArithmetic + Copy>
        DealProposal<Address, Balance, BlockNumber>
    {
        fn duration(&self) -> BlockNumber {
            self.end_block - self.start_block
        }

        fn total_storage_fee(&self) -> Option<u128> {
            // We need to convert into something to perform the calculation.
            // Generics trickery prevents us from doing it in a nice way.
            // <https://stackoverflow.com/questions/56081117/how-do-you-convert-between-substrate-specific-types-and-rust-primitive-types>
            Some(
                TryInto::<u128>::try_into(self.storage_price_per_block).ok()?
                    * TryInto::<u128>::try_into(self.duration()).ok()?,
            )
        }

        fn cid(&self) -> Result<Cid, ProposalError> {
            let cid = Cid::try_from(&self.piece_cid[..])
                .map_err(|e| ProposalError::InvalidPieceCid(e))?;
            Ok(cid)
        }
    }

    type DealProposalOf<T> =
        DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

    /// After Storage Client has successfully negotiated with the Storage Provider, they prepare a DealProposal,
    /// sign it with their signature and send to the Storage Provider.
    /// Storage Provider only after successful file transfer and verification of the data, calls an extrinsic `market.publish_storage_deals`.
    /// The extrinsic call is signed by the Storage Provider and Storage Client's signature is in the message.
    /// Based on that, Market Pallet can verify the signature and lock appropriate funds.
    #[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct ClientDealProposal<Address, Currency, BlockNumber, OffchainSignature> {
        pub proposal: DealProposal<Address, Currency, BlockNumber>,
        pub client_signature: OffchainSignature,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// [`BalanceTable`] is used to store balances for Storage Market Participants.
    /// Both Clients and Providers track their `free` and `locked` funds.
    /// * `free funds` can be added by `add_balance` method and withdrawn by `withdrawn_balance` method.
    /// * `free funds` are converted to `locked_funds` when staked as collateral for _Deals_.
    /// * `locked funds` cannot be withdrawn freely, first some process need to unlock it.
    /// Invariant must be held at all times:
    /// `account(MarketPallet).balance == all_accounts.map(|balance| balance[account]].locked + balance[account].free).sum()`
    #[pallet::storage]
    pub type BalanceTable<T: Config> =
        StorageMap<_, _, T::AccountId, BalanceEntry<BalanceOf<T>>, ValueQuery>;

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
    pub type Proposals<T: Config> =
        StorageMap<_, _, DealId, DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>>;

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
    /// If it's not, Storage Provider is slashed and Client refunded by [`Hooks::on_finalize`] .
    /// If it has been activated properly, it's just removed from the map.
    #[pallet::storage]
    pub type DealsForBlock<T: Config> = StorageMap<
        _,
        _,
        BlockNumberFor<T>,
        BoundedBTreeSet<DealId, T::MaxDealsPerBlock>,
        ValueQuery,
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
        /// Deal has been successfully published between a client and a provider.
        DealPublished {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },
        // Deal has been successfully activated.
        DealActivated {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },
        /// Deals were settled.
        DealsSettled {
            /// Deal IDs for those that were successfully settled.
            successful: BoundedVec<DealId, MaxSettleDeals<T>>,
            /// Deal IDs for those that were not successfully settled along with the respective error.
            unsuccessful: BoundedVec<(DealId, DealSettlementError), MaxSettleDeals<T>>,
        },
        /// Deal was slashed.
        DealSlashed(DealId),
    }

    /// Utility type to ensure that the bound for deal settlement is in sync.
    pub type MaxSettleDeals<T> = <T as Config>::MaxDeals;

    #[pallet::error]
    pub enum Error<T> {
        /// Market Participant tries to withdraw more
        /// funds than they have available on the Market, because:
        /// - they never deposited the amount they want to withdraw
        /// - the funds they deposited were locked as part of a deal
        InsufficientFreeFunds,
        /// `publish_storage_deals` was called with empty `deals` array.
        NoProposalsToBePublished,
        /// `publish_storage_deals` must be called by Storage Providers and it's a Provider of all of the deals.
        ProposalsNotPublishedByStorageProvider,
        /// `publish_storage_deals` call was supplied with `deals` which are all invalid.
        AllProposalsInvalid,
        /// `publish_storage_deals`'s core logic was invoked with a broken invariant that should be called by `validate_deals`.
        UnexpectedValidationError,
        /// There is more than 1 deal of this ID in the Sector.
        DuplicateDeal,
        /// Due to a programmer bug, bounds on Bounded data structures were incorrect so couldn't insert into them.
        DealPreconditionFailed,
        /// Tried to activate a deal which is not in the system.
        DealNotFound,
        /// Tried to activate a deal, but data doesn't make sense. Details are in the logs.
        DealActivationError,
        /// Sum of all of the deals piece sizes for a sector exceeds sector size.
        DealsTooLargeToFitIntoSector,
        /// Tried to activate too many deals at a given start_block.
        TooManyDealsPerBlock,
    }

    #[derive(RuntimeDebug)]
    pub enum DealActivationError {
        /// Deal was tried to be activated by a provider which does not own it
        InvalidProvider,
        /// Deal should have been activated earlier, it's too late
        StartBlockElapsed,
        /// Sector containing the deal will expire before the deal is supposed to end
        SectorExpiresBeforeDeal,
        /// Deal needs to be [`DealState::Published`] if it's to be activated
        InvalidDealState,
        /// Tried to activate a deal which is not in the system.
        DealNotFound,
        /// Tried to activate a deal which is not in the Pending Proposals
        DealNotPending,
    }

    // NOTE(@th7nder,18/06/2024):
    // would love to use `thiserror` but it's not supporting no_std environments yet
    // `thiserror-core` relies on rust nightly feature: error_in_core
    /// Errors related to [`DealProposal`] and [`ClientDealProposal`]
    /// This is error does not surface externally, only in the logs.
    /// Mostly used for Deal Validation [`Self::<T>::validate_deals`].
    #[derive(RuntimeDebug)]
    pub enum ProposalError {
        /// ClientDealProposal.client_signature did not match client's public key and data.
        WrongClientSignatureOnProposal,
        /// Provider of one of the deals is different than the Provider of the first deal.
        DifferentProvider,
        /// Deal's block_start > block_end, so it doesn't make sense.
        DealEndBeforeStart,
        /// Deal has to be [`DealState::Published`] when being Published
        DealNotPublished,
        /// Deal's duration must be within `Config::MinDealDuration` < `Config:MaxDealDuration`.
        DealDurationOutOfBounds,
        /// Deal's piece_cid is invalid.
        InvalidPieceCid(cid::Error),
    }

    // Clone and PartialEq required because of the BoundedVec<(DealId, DealSettlementError)>
    #[derive(RuntimeDebug, TypeInfo, Encode, Decode, Clone, PartialEq)]
    pub enum DealSettlementError {
        /// The deal is going to be slashed.
        SlashedDeal,
        /// The deal last update is in the future — i.e. `last_update_block > current_block`.
        FutureLastUpdate,
        /// The deal was not found.
        DealNotFound,
        /// The deal is too early to settle.
        EarlySettlement,
        /// The deal has expired
        ExpiredDeal,
    }

    /// Extrinsics exposed by the pallet
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Transfers `amount` of Balance from the `origin` to the Market Pallet account.
        /// It is marked as _free_ in the Market bookkeeping.
        /// Free balance can be withdrawn at any moment from the Market.
        pub fn add_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            BalanceTable::<T>::try_mutate(&caller, |balance| -> DispatchResult {
                balance.free = balance
                    .free
                    .checked_add(&amount)
                    .ok_or(ArithmeticError::Overflow)?;
                T::Currency::transfer(&caller, &Self::account_id(), amount, KeepAlive)?;

                Self::deposit_event(Event::<T>::BalanceAdded {
                    who: caller.clone(),
                    amount,
                });
                Ok(())
            })?;

            Ok(())
        }

        /// Transfers `amount` of Balance from the Market Pallet account to the `origin`.
        /// Only _free_ balance can be withdrawn.
        pub fn withdraw_balance(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            BalanceTable::<T>::try_mutate(&caller, |balance| -> DispatchResult {
                ensure!(balance.free >= amount, Error::<T>::InsufficientFreeFunds);
                balance.free = balance
                    .free
                    .checked_sub(&amount)
                    .ok_or(ArithmeticError::Underflow)?;
                // The Market Pallet account will be reaped if no one is participating in the market.
                T::Currency::transfer(&Self::account_id(), &caller, amount, AllowDeath)?;

                Self::deposit_event(Event::<T>::BalanceWithdrawn {
                    who: caller.clone(),
                    amount,
                });
                Ok(())
            })?;

            Ok(())
        }

        /// Settle pending deal payments for the given deal IDs.
        ///
        /// This function *should* only fully fail when a block was last updated after its `end_block` target.
        ///
        /// In other cases, the function will return two lists, the successful settlements and the unsuccessful ones.
        ///
        /// A settlement is only fully performed when a deal is active.
        /// If a deal is not active, its settlement is simply marked as successful but nothing happens.
        ///
        /// A settlement is unsuccessful when:
        /// * The deal was not found. The returned error is [`DealSettlementError::DealNotFound`].
        /// * The deal's start block is after the current block, meaning it's too early to settle the deal.
        ///   The returned error is [`DealSettlementError::EarlySettlement`].
        /// * The deal has been slashed. The returned error is [`DealSettlementError::SlashedDeal`].
        /// * The deal's last update is after the current block, meaning the deal's last update is in the future.
        ///   The returned error is [`DealSettlementError::FutureLastUpdate`].
        pub fn settle_deal_payments(
            origin: OriginFor<T>,
            // The original `deals` structure is a bitfield from fvm-ipld-bitfield
            deal_ids: BoundedVec<DealId, MaxSettleDeals<T>>,
        ) -> DispatchResult {
            // Anyone with gas can settle payments, so we just check if the origin is signed
            ensure_signed(origin)?;

            // INVARIANT: slashed deals cannot show up here because slashing is fully processed by `on_sector_terminate`

            let current_block = <frame_system::Pallet<T>>::block_number();

            let mut successful = BoundedVec::<_, MaxSettleDeals<T>>::new();
            let mut unsuccessful = BoundedVec::<_, MaxSettleDeals<T>>::new();

            for deal_id in deal_ids {
                // If the deal is not found, we register an error and move on
                // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1225-L1231
                let Some(mut deal_proposal) = Proposals::<T>::get(deal_id) else {
                    log::error!(target: LOG_TARGET, "deal not found — deal_id: {}", deal_id);
                    // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
                    let _ = unsuccessful.try_push((deal_id, DealSettlementError::DealNotFound));
                    continue;
                };

                // Deal isn't possibly valid yet
                // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1255-L1264
                if deal_proposal.start_block > current_block {
                    // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
                    let _ = unsuccessful.try_push((deal_id, DealSettlementError::EarlySettlement));
                    continue;
                }

                // If the deal is not active (i.e. unpublished or published), there's nothing to settle
                // https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1225-L1231
                let DealState::Active(ref mut active_deal_state) = deal_proposal.state else {
                    // If a deal is not published, there's nothing to settle
                    // If a deal is published, but not active, it's supposed to be removed by cron/hooks

                    // NOTE(@jmg-duarte,28/06/2024): maybe we should handle deals where deal_proposal.start_block < current_block — i.e. expired

                    // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
                    let _ = successful.try_push(deal_id);
                    continue;
                };

                // If the last updated block is in the future, return an error
                if let Some(last_updated_block) = active_deal_state.last_updated_block {
                    if last_updated_block > current_block {
                        log::error!(target: LOG_TARGET,
                            "last_updated_block for deal is in the future — deal_id: {}, last_updated_block: {:?}",
                            deal_id,
                            last_updated_block
                        );
                        // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
                        let _ =
                            unsuccessful.try_push((deal_id, DealSettlementError::FutureLastUpdate));
                        continue;
                    }
                }

                // If we never settled, the duration starts at `start_block`
                let last_settled_block = active_deal_state
                    .last_updated_block
                    .unwrap_or(deal_proposal.start_block);

                if last_settled_block > deal_proposal.end_block {
                    // If the code reaches this, it's a big whoops
                    log::error!(target: LOG_TARGET, "the last settled block cannot be bigger than the end block — last_settled_block: {:?}, end_block: {:?}",
                        last_settled_block, deal_proposal.end_block);
                    return Err(DispatchError::Corruption);
                }

                let (block_to_settle, complete_deal) = {
                    if current_block >= deal_proposal.end_block {
                        // The deal has been completed, as such, we'll remove it later on
                        (deal_proposal.end_block, true)
                    } else {
                        (current_block, false)
                    }
                };

                // If an error happens when converting here we have more to worry about than completing all settlements
                let deal_settlement_amount: BalanceOf<T> = {
                    // There's no great way to avoid the repeated code without macros or more generics magic
                    // ArithmeticError::Overflow used as `duration` and `storage_price_per_block` can only be positive
                    let duration: u128 = (block_to_settle - last_settled_block)
                        .try_into()
                        .map_err(|_| DispatchError::Arithmetic(ArithmeticError::Overflow))?;
                    let storage_price_per_block: u128 = deal_proposal
                        .storage_price_per_block
                        .try_into()
                        .map_err(|_| DispatchError::Arithmetic(ArithmeticError::Overflow))?;

                    (duration * storage_price_per_block)
                        .try_into()
                        .map_err(|_| DispatchError::Arithmetic(ArithmeticError::Overflow))
                }?;

                perform_storage_payment::<T>(
                    &deal_proposal.client,
                    &deal_proposal.provider,
                    deal_settlement_amount,
                )?;

                // NOTE(@jmg-duarte,28/06/2024): Maybe emit an event when the table is updated?
                if complete_deal {
                    Proposals::<T>::remove(deal_id);
                } else {
                    // Otherwise, we update the proposal — `last_updated_block`
                    active_deal_state.last_updated_block = Some(current_block);
                    Proposals::<T>::insert(deal_id, deal_proposal);
                }
                // SAFETY: Always succeeds because the upper bound on the vecs should be the same as the input vec
                let _ = successful.try_push(deal_id);
            }

            Self::deposit_event(Event::<T>::DealsSettled {
                successful,
                unsuccessful,
            });

            Ok(())
        }

        /// Publish a new set of storage deals (not yet included in a sector).
        /// It saves valid deals as [`DealState::Published`] and locks up client fees and provider's collaterals.
        /// Locked up balances cannot be withdrawn until a deal is terminated.
        /// All of the deals must belong to a single Storage Provider.
        /// It is permissive, if some of the deals are correct and some are not, it emits events for valid deals.
        /// On success emits [`Event::<T>::DealPublished`] for each successful deal.
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
            let provider = ensure_signed(origin)?;
            let (valid_deals, total_provider_lockup) =
                Self::validate_deals(provider.clone(), deals)?;

            // Lock up funds for the clients and emit events
            for deal in valid_deals.into_iter() {
                // PRE-COND: always succeeds, validated by `validate_deals`
                let client_fee: BalanceOf<T> = deal
                    .total_storage_fee()
                    .ok_or(Error::<T>::UnexpectedValidationError)?
                    .try_into()
                    .map_err(|_| Error::<T>::UnexpectedValidationError)?;

                BalanceTable::<T>::try_mutate(&deal.client, |balance| -> DispatchResult {
                    // PRE-COND: always succeeds, validated by `validate_deals`
                    balance.free = balance
                        .free
                        .checked_sub(&client_fee)
                        .ok_or(ArithmeticError::Underflow)?;
                    balance.locked = balance
                        .locked
                        .checked_add(&client_fee)
                        .ok_or(ArithmeticError::Overflow)?;

                    Ok(())
                })?;

                let deal_id = Self::generate_deal_id();

                Self::deposit_event(Event::<T>::DealPublished {
                    client: deal.client.clone(),
                    provider: provider.clone(),
                    deal_id,
                });
                let mut deals_for_block = DealsForBlock::<T>::get(&deal.start_block);
                deals_for_block.try_insert(deal_id).map_err(|_| {
                    log::error!("there is not enough space to activate all of the deals at the given block {:?}", deal.start_block);
                    Error::<T>::TooManyDealsPerBlock
                })?;
                DealsForBlock::<T>::insert(deal.start_block, deals_for_block);
                Proposals::<T>::insert(deal_id, deal);
            }

            // Lock up funds for the Storage Provider
            // PRE-COND: always succeeds, validated by `validate_deals`
            BalanceTable::<T>::try_mutate(&provider, |balance| -> DispatchResult {
                balance.free = balance
                    .free
                    .checked_sub(&total_provider_lockup)
                    .ok_or(ArithmeticError::Underflow)?;
                balance.locked = balance
                    .locked
                    .checked_add(&total_provider_lockup)
                    .ok_or(ArithmeticError::Overflow)?;
                Ok(())
            })?;

            Ok(())
        }
    }

    /// Functions exposed by the pallet
    impl<T: Config> Pallet<T> {
        /// Account Id of the Market
        ///
        /// This actually does computation.
        /// If you need to keep using it, make sure you cache it and call it once.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// Validates the signature of the given data with the provided signer's account ID.
        ///
        /// # Errors
        ///
        /// This function returns a [`WrongSignature`](crate::Error::WrongClientSignatureOnProposal)
        /// error if the signature is invalid or the verification process fails.
        pub fn validate_signature(
            data: &[u8],
            signature: &T::OffchainSignature,
            signer: &T::AccountId,
        ) -> Result<(), ProposalError> {
            if signature.verify(data, &signer) {
                return Ok(());
            }

            // NOTE: for security reasons modern UIs implicitly wrap the data requested to sign into
            // <Bytes></Bytes>, that's why we support both wrapped and raw versions.
            let prefix = b"<Bytes>";
            let suffix = b"</Bytes>";
            let mut wrapped = Vec::with_capacity(data.len() + prefix.len() + suffix.len());
            wrapped.extend(prefix);
            wrapped.extend(data);
            wrapped.extend(suffix);

            ensure!(
                signature.verify(&*wrapped, &signer),
                ProposalError::WrongClientSignatureOnProposal
            );

            Ok(())
        }

        /// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1370>
        fn compute_commd<'a>(
            _proposals: impl IntoIterator<Item = &'a DealProposalOf<T>>,
            _sector_type: RegisteredSealProof,
        ) -> Result<Cid, DispatchError> {
            // TODO(@th7nder,#92,21/06/2024):
            // https://github.com/filecoin-project/rust-fil-proofs/blob/daec42b64ae6bf9a537545d5f116d57b9a29cc11/filecoin-proofs/src/pieces.rs#L85
            let cid = Cid::new_v1(
                CID_CODEC,
                Code::Blake2b256.digest(b"placeholder-to-be-done"),
            );

            Ok(cid)
        }

        /// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1388>
        fn validate_deals_for_sector(
            deals: &BoundedVec<(DealId, DealProposalOf<T>), ConstU32<32>>,
            provider: &T::AccountId,
            sector_number: SectorNumber,
            sector_expiry: BlockNumberFor<T>,
            sector_activation: BlockNumberFor<T>,
            sector_size: SectorSize,
        ) -> DispatchResult {
            let mut total_deal_space = 0;
            for (deal_id, deal) in deals {
                Self::validate_deal_can_activate(deal, provider, sector_expiry, sector_activation)
                    .map_err(|e| {
                        log::error!(target: LOG_TARGET, "deal {} cannot be activated, because: {:?}", *deal_id, e);
                        Error::<T>::DealActivationError }
                    )?;
                total_deal_space += deal.piece_size;
            }

            ensure!(total_deal_space <= sector_size.bytes(), {
                log::error!(target: LOG_TARGET, "cannot fit all of the deals into sector {}, {} < {}", sector_number, total_deal_space, sector_size.bytes());
                Error::<T>::DealsTooLargeToFitIntoSector
            });

            Ok(())
        }

        /// <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1570>
        fn validate_deal_can_activate(
            deal: &DealProposalOf<T>,
            provider: &T::AccountId,
            sector_expiry: BlockNumberFor<T>,
            sector_activation: BlockNumberFor<T>,
        ) -> Result<(), DealActivationError> {
            ensure!(
                *provider == deal.provider,
                DealActivationError::InvalidProvider
            );
            ensure!(
                deal.state == DealState::Published,
                DealActivationError::InvalidDealState
            );
            ensure!(
                sector_activation <= deal.start_block,
                DealActivationError::StartBlockElapsed
            );
            ensure!(
                sector_expiry >= deal.end_block,
                DealActivationError::SectorExpiresBeforeDeal
            );

            // Confirm the deal is in the pending proposals set.
            // It will be removed from this queue later, during cron.
            // Failing this check is an internal invariant violation.
            // The pending deals set exists to prevent duplicate proposals.
            // It should be impossible to have a proposal, no deal state, and not be in pending deals.
            let hash = Self::hash_proposal(&deal);
            ensure!(
                PendingProposals::<T>::get().contains(&hash),
                DealActivationError::DealNotPending
            );

            Ok(())
        }

        fn proposals_for_deals(
            deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
        ) -> Result<
            BoundedVec<(DealId, DealProposalOf<T>), ConstU32<MAX_SECTORS_PER_CALL>>,
            DispatchError,
        > {
            let mut unique_deals: BoundedBTreeSet<DealId, ConstU32<MAX_SECTORS_PER_CALL>> =
                BoundedBTreeSet::new();
            let mut proposals = BoundedVec::new();
            for deal_id in deal_ids {
                ensure!(!unique_deals.contains(&deal_id), {
                    log::error!(target: LOG_TARGET, "deal {} is duplicated", deal_id);
                    Error::<T>::DuplicateDeal
                });

                // PRE-COND: always succeeds, unique_deals has the same boundary as sector.deal_ids[]
                unique_deals.try_insert(deal_id).map_err(|deal_id| {
                    log::error!(target: LOG_TARGET, "failed to insert deal {}", deal_id);
                    Error::<T>::DealPreconditionFailed
                })?;

                let proposal: DealProposalOf<T> =
                    Proposals::<T>::try_get(&deal_id).map_err(|_| {
                        log::error!(target: LOG_TARGET, "deal {} not found", deal_id);
                        Error::<T>::DealNotFound
                    })?;

                // PRE-COND: always succeeds, unique_deals has the same boundary as sector.deal_ids[]
                proposals
                    .try_push((deal_id, proposal))
                    .map_err(|_| {
                            log::error!(target: LOG_TARGET, "failed to insert deal {} into proposals", deal_id);
                            Error::<T>::DealPreconditionFailed
                        }
                    )?;
            }

            Ok(proposals)
        }

        fn generate_deal_id() -> DealId {
            let ret = NextDealId::<T>::get();
            let next = ret
                .checked_add(1)
                .expect("we ran out of free deal ids, not ideal");
            NextDealId::<T>::set(next);
            ret
        }

        fn sanity_check(
            deal: &ClientDealProposal<
                T::AccountId,
                BalanceOf<T>,
                BlockNumberFor<T>,
                T::OffchainSignature,
            >,
            provider: &T::AccountId,
        ) -> Result<(), ProposalError> {
            Self::validate_signature(
                &Encode::encode(&deal.proposal),
                &deal.client_signature,
                &deal.proposal.client,
            )?;

            // Ensure the Piece's Cid is parsable and valid
            let _ = deal.proposal.cid()?;

            ensure!(
                deal.proposal.provider == *provider,
                ProposalError::DifferentProvider
            );

            ensure!(
                deal.proposal.start_block < deal.proposal.end_block,
                ProposalError::DealEndBeforeStart
            );

            ensure!(
                deal.proposal.state == DealState::Published,
                ProposalError::DealNotPublished
            );

            let min_dur = T::BlocksPerDay::get() * T::MinDealDuration::get();
            let max_dur = T::BlocksPerDay::get() * T::MaxDealDuration::get();
            ensure!(
                deal.proposal.duration() >= min_dur && deal.proposal.duration() <= max_dur,
                ProposalError::DealDurationOutOfBounds
            );

            // TODO(@th7nder,#81,18/06/2024): figure out the minimum collateral limits
            // <https://spec.filecoin.io/#section-systems.filecoin_markets.onchain_storage_market.storage_market_actor.storage-deal-collateral>

            Ok(())
        }

        fn validate_deals(
            caller: T::AccountId,
            deals: BoundedVec<
                ClientDealProposal<
                    T::AccountId,
                    BalanceOf<T>,
                    BlockNumberFor<T>,
                    T::OffchainSignature,
                >,
                T::MaxDeals,
            >,
        ) -> Result<
            (
                Vec<DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>>,
                BalanceOf<T>,
            ),
            DispatchError,
        > {
            ensure!(deals.len() > 0, Error::<T>::NoProposalsToBePublished);

            // All deals should have the same provider, so get it once.
            let provider = deals[0].proposal.provider.clone();
            ensure!(
                caller == provider,
                Error::<T>::ProposalsNotPublishedByStorageProvider
            );

            // TODO(@th7nder,#87,17/06/2024): validate a Storage Provider's Account (whether the account was registered as Storage Provider)

            let mut total_client_lockup: BoundedBTreeMap<T::AccountId, BalanceOf<T>, T::MaxDeals> =
                BoundedBTreeMap::new();
            let mut total_provider_lockup: BalanceOf<T> = Default::default();
            let mut message_proposals: BoundedBTreeSet<T::Hash, T::MaxDeals> =
                BoundedBTreeSet::new();

            let valid_deals = deals.into_iter().enumerate().filter_map(|(idx, deal)| {
                    if let Err(e) = Self::sanity_check(&deal, &provider) {
                        log::error!(target: LOG_TARGET, "insane deal: idx {}, error: {:?}", idx, e);
                        return None;
                    }

                    // there is no Entry API in BoundedBTreeMap
                    let mut client_lockup =
                        if let Some(client_lockup) = total_client_lockup.get(&deal.proposal.client) {
                            *client_lockup
                        } else {
                            Default::default()
                        };
                    let client_fees: BalanceOf<T> = deal.proposal.total_storage_fee()?.try_into().ok()?;
                    client_lockup = client_lockup.checked_add(&client_fees)?;

                    let client_balance = BalanceTable::<T>::get(&deal.proposal.client);
                    if client_lockup > client_balance.free {
                        log::error!(target: LOG_TARGET, "invalid deal: client {:?} not enough free balance {:?} < {:?} to cover deal idx: {}",
                            deal.proposal.client, client_balance.free, client_lockup, idx);
                        return None;
                    }

                    let mut provider_lockup = total_provider_lockup;
                    provider_lockup = provider_lockup.checked_add(&deal.proposal.provider_collateral)?;

                    let provider_balance = BalanceTable::<T>::get(&deal.proposal.provider);
                    if provider_lockup > provider_balance.free {
                        log::error!(target: LOG_TARGET, "invalid deal: storage provider {:?} not enough free balance {:?} < {:?} to cover deal idx: {}",
                            deal.proposal.provider, provider_balance.free, provider_lockup, idx);
                        return None;
                    }

                    let hash = Self::hash_proposal(&deal.proposal);
                    let duplicate_in_state = PendingProposals::<T>::get().contains(&hash);
                    let duplicate_in_message = message_proposals.contains(&hash);
                    if duplicate_in_state || duplicate_in_message {
                        log::error!(target: LOG_TARGET, "invalid deal: cannot publish duplicate deal idx: {}", idx);
                        return None;
                    }
                    let mut pending = PendingProposals::<T>::get();
                    if let Err(e) = pending.try_insert(hash) {
                        log::error!(target: LOG_TARGET, "cannot publish: too many pending deal proposals, wait for them to be expired/activated, deal idx: {}, err: {:?}", idx, e);
                        return None;
                    }
                    PendingProposals::<T>::set(pending);
                    // PRE-COND: always succeeds, as there cannot be more deals than T::MaxDeals and this the size of the set
                    message_proposals.try_insert(hash).ok()?;
                    // PRE-COND: always succeeds as there cannot be more clients than T::MaxDeals
                    total_client_lockup.try_insert(deal.proposal.client.clone(), client_lockup)
                        .ok()?;
                    total_provider_lockup = provider_lockup;
                    Some(deal.proposal)
                }).collect::<Vec<_>>();
            ensure!(valid_deals.len() > 0, Error::<T>::AllProposalsInvalid);

            Ok((valid_deals, total_provider_lockup))
        }

        // Used for deduplication purposes
        // We don't want to store another BTreeSet of DealProposals
        // We only care about hashes.
        // It is not an associated function, because T::Hashing is hard to use inside of there.
        pub(crate) fn hash_proposal(
            proposal: &DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
        ) -> T::Hash {
            let bytes = Encode::encode(proposal);
            T::Hashing::hash(&bytes)
        }

        fn slash_and_burn(account_id: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
            BalanceTable::<T>::try_mutate(account_id, |balance| -> DispatchResult {
                let locked = balance
                    .locked
                    .checked_sub(&amount)
                    .ok_or(ArithmeticError::Underflow)?;
                balance.locked = locked;
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

    impl<T: Config> Market<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
        /// Verifies a given set of storage deals is valid for sectors being PreCommitted.
        /// Computes UnsealedCID (CommD) for each sector or None for Committed Capacity sectors.
        /// Currently UnsealedCID is hardcoded as we `compute_commd` remains unimplemented because of #92.
        fn verify_deals_for_activation(
            storage_provider: &T::AccountId,
            sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, ConstU32<MAX_SECTORS_PER_CALL>>,
        ) -> Result<BoundedVec<Option<Cid>, ConstU32<MAX_SECTORS_PER_CALL>>, DispatchError>
        {
            let curr_block = System::<T>::block_number();
            let mut unsealed_cids = BoundedVec::new();
            for sector in sector_deals {
                let proposals = Self::proposals_for_deals(sector.deal_ids)?;
                let sector_size = sector.sector_type.sector_size();
                Self::validate_deals_for_sector(
                    &proposals,
                    storage_provider,
                    sector.sector_number,
                    sector.sector_expiry,
                    curr_block,
                    sector_size,
                )?;

                // Sealing a Sector without Deals, Committed Capacity Only.
                let commd = if proposals.is_empty() {
                    None
                } else {
                    Some(Self::compute_commd(
                        proposals.iter().map(|(_, deal)| deal),
                        sector.sector_type,
                    )?)
                };

                // PRE-COND: can't fail, unsealed_cids<_, X> == BoundedVec<_ X> == sector_deals<_, X>
                unsealed_cids
                    .try_push(commd)
                    .map_err(|_| "programmer error, there should be space for Cids")?;
            }

            Ok(unsealed_cids)
        }

        /// Activate a set of deals grouped by sector, returning the size and
        /// extra info about verified deals.
        /// Sectors' deals are activated in parameter-defined order.
        /// Each sector's deals are activated or fail as a group, but independently of other sectors.
        /// Note that confirming all deals fit within a sector is the caller's responsibility
        /// (and is implied by confirming the sector's data commitment is derived from the deal pieces).
        fn activate_deals(
            storage_provider: &T::AccountId,
            sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, ConstU32<MAX_SECTORS_PER_CALL>>,
            compute_cid: bool,
        ) -> Result<
            BoundedVec<ActiveSector<T::AccountId>, ConstU32<MAX_SECTORS_PER_CALL>>,
            DispatchError,
        > {
            // TODO(@th7nder,#87,17/06/2024): validate a Storage Provider's Account (whether the account was registered as Storage Provider)
            let mut activations = BoundedVec::new();
            let curr_block = System::<T>::block_number();
            let mut activated_deal_ids: BoundedBTreeSet<
                DealId,
                ConstU32<MAX_DEALS_FOR_ALL_SECTORS>,
            > = BoundedBTreeSet::new();

            let mut pending_proposals = PendingProposals::<T>::get();
            for sector in sector_deals {
                let proposals = Self::proposals_for_deals(sector.deal_ids)?;
                let sector_size = sector.sector_type.sector_size();
                if let Err(e) = Self::validate_deals_for_sector(
                    &proposals,
                    storage_provider,
                    sector.sector_number,
                    sector.sector_expiry,
                    curr_block,
                    sector_size,
                ) {
                    log::error!(
                        "failed to activate sector: {}, skipping... {:?}",
                        sector.sector_number,
                        e
                    );
                    continue;
                }

                let data_commitment = if compute_cid && !proposals.is_empty() {
                    Some(Self::compute_commd(
                        proposals.iter().map(|(_, deal)| deal),
                        sector.sector_type,
                    )?)
                } else {
                    None
                };

                let mut activated_deals: BoundedVec<_, ConstU32<MAX_DEALS_PER_SECTOR>> =
                    BoundedVec::new();
                for (deal_id, mut proposal) in proposals {
                    // Make it Active! This is what's this function is about in the end.
                    pending_proposals.remove(&Self::hash_proposal(&proposal));
                    proposal.state =
                        DealState::Active(ActiveDealState::new(sector.sector_number, curr_block));

                    activated_deals
                        .try_push(ActiveDeal {
                            client: proposal.client.clone(),
                            piece_cid: proposal.cid().map_err(|e| {
                                log::error!(
                                    "there is invalid cid saved on-chain for deal: {}, {:?}",
                                    deal_id,
                                    e
                                );
                                Error::<T>::DealPreconditionFailed
                            })?,
                            piece_size: proposal.piece_size,
                        })
                        .map_err(|_| {
                            log::error!("failed to insert into `activated`, programmer's error");
                            Error::<T>::DealPreconditionFailed
                        })?;
                    activated_deal_ids.try_insert(deal_id).map_err(|_| {
                        log::error!(
                            "failed to insert into `activated_deal_ids`, programmer's error"
                        );
                        Error::<T>::DealPreconditionFailed
                    })?;

                    Self::deposit_event(Event::<T>::DealActivated {
                        deal_id,
                        client: proposal.client.clone(),
                        provider: proposal.provider.clone(),
                    });
                    Proposals::<T>::insert(deal_id, proposal);
                }

                activations
                    .try_push(ActiveSector {
                        active_deals: activated_deals,
                        unsealed_cid: data_commitment,
                    })
                    .map_err(|_| Error::<T>::DealPreconditionFailed)?;
            }

            PendingProposals::<T>::set(pending_proposals);
            Ok(activations)
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

        /// Called for each block after every extrinsic has been called.
        /// This is a slasher for Deals that have been Published, but Storage Provider failed to activate them.
        /// It scans for Deals that were supposed to be activated in a given block, when registered in `publish_storage_deals`.
        /// When a deal is not [`DealState::Active`], it refunds all the funds to the client and burns provider's collateral.
        /// When a deal has been activated, it just removes it from data structures used for tracking.
        /// This function should not fail at any point, if it fails, it's a bug.
        ///
        /// ### Technical notes
        /// Should be deterministic, bounded and have minimal time complexity.
        /// <https://paritytech.github.io/polkadot-sdk/master/frame_support/traits/trait.Hooks.html#summary>
        fn on_finalize(current_block: BlockNumberFor<T>) {
            let deal_ids = DealsForBlock::<T>::get(&current_block);
            if deal_ids.is_empty() {
                log::info!(target: LOG_TARGET, "no deals to process in block: {:?}", current_block);
                return;
            }

            // INVARIANT: every deal in deal_ids is unique.
            // PRE-COND: deal validation has been performed by `publish_storage_deals`.
            let mut pending_proposals = PendingProposals::<T>::get();
            for deal_id in deal_ids {
                let proposal = match Proposals::<T>::try_get(&deal_id) {
                    Ok(proposal) => proposal,
                    // Proposal might have been cleaned up by manual settlement or termination prior to reaching
                    // this scheduled block. nothing more to do for this deal
                    Err(_) => continue,
                };

                match &proposal.state {
                    DealState::Published => {
                        assert!(
                            proposal.start_block == current_block,
                            "deals are scheduled to be checked only at their start block"
                        );
                        // Deal has not been activated, time to slash!
                        // PRE-COND: deal cannot make to this stage without being validated and proper funds allocated
                        let _ = BalanceTable::<T>::try_mutate(
                            &proposal.client,
                            |balance| -> DispatchResult {
                                let client_fee: BalanceOf<T> = proposal
                                    .total_storage_fee()
                                    .ok_or(Error::<T>::UnexpectedValidationError)?
                                    .try_into()
                                    .map_err(|_| Error::<T>::UnexpectedValidationError)?;
                                balance.free = balance
                                    .free
                                    .checked_add(&client_fee)
                                    .ok_or(ArithmeticError::Overflow)?;
                                balance.locked = balance
                                    .locked
                                    .checked_sub(&client_fee)
                                    .ok_or(ArithmeticError::Underflow)?;
                                Ok(())
                            },
                        );

                        log::info!(
                            "Slashing {:?} for not activating a deal: {}",
                            proposal.provider,
                            deal_id
                        );
                        // PRE-COND: deal MUST BE validated and the proper funds allocated
                        let _ =
                            Self::slash_and_burn(&proposal.provider, proposal.provider_collateral);
                    }
                    DealState::Active(_) => {
                        log::info!(
                            "Deal {} has been properly activated before, all good.",
                            deal_id
                        );
                    }
                }

                // Deal has been processed, no need to process it twice.
                Proposals::<T>::remove(&deal_id);
                // PRE-COND: all deals in DealsPerBlock are published.
                // All Published deals are hashed and added to [`PendingProposals`].
                let _ = pending_proposals.remove(&Self::hash_proposal(&proposal));
            }

            PendingProposals::<T>::set(pending_proposals);
            DealsForBlock::<T>::remove(&current_block);
        }
    }

    /// Moves the provided `amount` from the `client`'s locked funds, to the provider's `free` funds.
    ///
    /// # Pre-Conditions
    /// * The client MUST have the necessary funds locked.
    fn perform_storage_payment<T: Config>(
        client: &T::AccountId,
        provider: &T::AccountId,
        amount: BalanceOf<T>,
    ) -> DispatchResult {
        // These should have been checked when locking funds
        BalanceTable::<T>::try_mutate(client, |balance| -> DispatchResult {
            let locked = balance
                .locked
                .checked_sub(&amount)
                .ok_or(ArithmeticError::Underflow)?;
            balance.locked = locked;
            Ok(())
        })?;

        BalanceTable::<T>::try_mutate(provider, |balance| -> DispatchResult {
            let free = balance
                .free
                .checked_add(&amount)
                .ok_or(ArithmeticError::Overflow)?;
            balance.free = free;
            Ok(())
        })?;

        Ok(())
    }
}
