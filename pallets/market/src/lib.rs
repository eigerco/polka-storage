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
            ReservableCurrency,
        },
        PalletId,
    };
    use frame_system::{pallet_prelude::*, Config as SystemConfig, Pallet as System};
    use multihash_codetable::{Code, MultihashDigest};
    use scale_info::TypeInfo;
    use sp_arithmetic::traits::BaseArithmetic;
    use sp_std::vec::Vec;

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

    // TODO(@th7nder,17/06/2024): this is likely to be extracted into primitives/ package
    type DealId = u64;

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

        /// How many deals can be activated in a single batch.
        #[pallet::constant]
        type MaxSectorsForActivation: Get<u32>;
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
        /// Deal has been negotiated off-chain and is being proposed via `publish_storage_deals`.
        Unpublished,
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
        sector_number: SectorNumber,

        /// At which block (time) the deal's sector has been activated.
        sector_start_block: BlockNumber,
        last_updated_block: Option<BlockNumber>,

        /// When the deal was last slashed, can be never.
        slash_block: Option<BlockNumber>,
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
        /// It goes: `Unpublished` -> `Published` -> `Active`
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
            let cid =
                Cid::try_from(&self.piece_cid[..]).map_err(|e| ProposalError::InvalidCid(e))?;
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

    // TODO(@th7nder,20/06/2024): this DOES NOT belong here. it should be somewhere else.
    #[allow(non_camel_case_types)]
    #[derive(Debug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
    pub enum RegisteredSealProof {
        StackedDRG2KiBV1P1,
    }

    impl RegisteredSealProof {
        pub fn sector_size(&self) -> SectorSize {
            SectorSize::_2KiB
        }
    }

    /// SectorSize indicates one of a set of possible sizes in the network.
    #[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq, Copy)]
    pub enum SectorSize {
        _2KiB,
    }

    impl SectorSize {
        /// <https://github.com/filecoin-project/ref-fvm/blob/5659196fa94accdf1e7f10e00586a8166c44a60d/shared/src/sector/mod.rs#L40>
        pub fn bytes(&self) -> u64 {
            match self {
                SectorSize::_2KiB => 2 << 10,
            }
        }
    }

    // TODO(@th7nder,20/06/2024): this DOES not belong here. it should be somewhere else.
    pub type SectorNumber = u64;

    #[derive(Debug, Decode, Encode, TypeInfo, Eq, PartialEq, Clone)]
    pub struct SectorDeal<BlockNumber> {
        pub sector_number: SectorNumber,
        pub sector_expiry: BlockNumber,
        pub sector_type: RegisteredSealProof,
        pub deal_ids: BoundedVec<DealId, ConstU32<128>>,
    }
    // verify_deals_for_activation is called by Storage Provider Pallllllet!
    // it's not an extrinsic then?

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type BalanceTable<T: Config> =
        StorageMap<_, _, T::AccountId, BalanceEntry<BalanceOf<T>>, ValueQuery>;

    #[pallet::storage]
    pub type NextDealId<T: Config> = StorageValue<_, DealId, ValueQuery>;

    #[pallet::storage]
    pub type Proposals<T: Config> =
        StorageMap<_, _, DealId, DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>>;

    #[pallet::storage]
    pub type PendingProposals<T: Config> =
        StorageValue<_, BoundedBTreeSet<T::Hash, T::MaxDeals>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Market Participant deposited free balance to the Market Account
        BalanceAdded {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Market Participant withdrawn their free balance from the Market Account
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
    }

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
        DuplicateDeal,
        DealPreconditionFailed,
        DealNotFound,
        DealActivationError,
        DealsTooLargeToFitIntoSector,
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
        WrongSignature,
        /// Provider of one of the deals is different than the Provider of the first deal.
        DifferentProvider,
        /// Deal's block_start > block_end, so it doesn't make sense.
        EndBeforeStart,
        /// Deal has to be [`DealState::Unpublished`] when being Published
        NotUnpublished,
        /// Deal's duration must be within `Config::MinDealDuration` < `Config:MaxDealDuration`.
        DurationOutOfBounds,
        /// Deal's piece_cid is invalid.
        InvalidCid(cid::Error),
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
            for mut deal in valid_deals.into_iter() {
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

                deal.state = DealState::Published;
                let deal_id = Self::generate_deal_id();

                Self::deposit_event(Event::<T>::DealPublished {
                    client: deal.client.clone(),
                    provider: provider.clone(),
                    deal_id,
                });
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
        /// This function returns a [`WrongSignature`](crate::Error::WrongSignature) error if the
        /// signature is invalid or the verification process fails.
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
                ProposalError::WrongSignature
            );

            Ok(())
        }

        /// Verifies a given set of storage deals is valid for sectors being PreCommitted.
        /// Computes UnsealedCID (CommD) for each sector or None for Committed Capacity sectors..
        /// Currently UnsealedCID is hardcoded as we `compute_commd` remains unimplemented because of #92.
        pub fn verify_deals_for_activation(
            storage_provider: &T::AccountId,
            sector_deals: BoundedVec<SectorDeal<BlockNumberFor<T>>, T::MaxSectorsForActivation>,
        ) -> Result<BoundedVec<Option<Cid>, T::MaxSectorsForActivation>, DispatchError> {
            // TODO:
            // - primitives
            // - trait in primitives
            // - docs
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

            Ok(())
        }

        fn proposals_for_deals(
            deal_ids: BoundedVec<DealId, ConstU32<128>>,
        ) -> Result<BoundedVec<(DealId, DealProposalOf<T>), ConstU32<32>>, DispatchError> {
            let mut unique_deals: BoundedBTreeSet<DealId, ConstU32<32>> = BoundedBTreeSet::new();
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
                ProposalError::EndBeforeStart
            );

            ensure!(
                deal.proposal.state == DealState::Unpublished,
                ProposalError::NotUnpublished
            );

            let min_dur = T::BlocksPerDay::get() * T::MinDealDuration::get();
            let max_dur = T::BlocksPerDay::get() * T::MaxDealDuration::get();
            ensure!(
                deal.proposal.duration() >= min_dur && deal.proposal.duration() <= max_dur,
                ProposalError::DurationOutOfBounds
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

            // TODO(@th7nder,#87,17/06/2024): validate a Storage Provider's Account (whether the account was registerd as Storage Provider)

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

                    let hash = Self::hash_proposal(&deal);
                    let duplicate_in_state = PendingProposals::<T>::get().contains(&hash);
                    let duplicate_in_message = message_proposals.contains(&hash);
                    if duplicate_in_state || duplicate_in_message {
                        log::error!(target: LOG_TARGET, "invalid deal: cannot publish duplicate deal idx: {}", idx);
                        return None;
                    }
                    if let Err(e) = PendingProposals::<T>::get().try_insert(hash) {
                        log::error!(target: LOG_TARGET, "cannot publish: too many pending deal proposals, wait for them to be expired/activated, deal idx: {}, err: {:?}", idx, e);
                        return None;
                    }
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
        // We don't want to store another BTreeSet of ClientDealProposals
        // We only care about hashes.
        // It is not an associated function, because T::Hashing is hard to use inside of there.
        fn hash_proposal(
            proposal: &ClientDealProposal<
                T::AccountId,
                BalanceOf<T>,
                BlockNumberFor<T>,
                T::OffchainSignature,
            >,
        ) -> T::Hash {
            let bytes = Encode::encode(proposal);
            T::Hashing::hash(&bytes)
        }
    }
}
