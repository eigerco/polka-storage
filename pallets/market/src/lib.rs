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

    use cid::{multihash::Multihash, Cid};
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
    use frame_system::{pallet_prelude::*, Config as SystemConfig};
    use scale_info::TypeInfo;
    use sp_arithmetic::traits::BaseArithmetic;
    use sp_std::vec::Vec;

    /// Allows to extract Balance of an account via the Config::Currency associated type.
    /// BalanceOf is a sophisticated way of getting an u128.
    type BalanceOf<T> =
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
        sector_number: u128,

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
        // We use BoundedVec here, as cid::Cid do not implement `TypeInfo`, so it cannot be saved into the Runtime Storage.
        // It maybe doable using newtype pattern, however not sure how the UI on the frontend side would handle that anyways.
        // There is Encode/Decode implementation though, through the feature flag: `scale-codec`.
        piece_cid: BoundedVec<u8, ConstU32<128>>,
        piece_size: u64,
        /// Storage Client's Account Id
        client: Address,
        /// Storage Provider's Account Id
        provider: Address,

        /// Arbitrary client chosen label to apply to the deal
        label: BoundedVec<u8, ConstU32<128>>,

        /// Nominal start block. Deal payment is linear between StartBlock and EndBlock,
        /// with total amount StoragePricePerBlock * (EndBlock - StartBlock).
        /// Storage deal must appear in a sealed (proven) sector no later than StartBlock,
        /// otherwise it is invalid.
        start_block: BlockNumber,
        /// When the Deal is supposed to end.
        end_block: BlockNumber,
        /// `Deal` can be terminated early, by `on_sectors_terminate`.
        /// Before that, a Storage Provider can payout it's earned fees by calling `on_settle_deal_payments`.
        /// `on_settle_deal_payments` must know how much money it can payout, so it's related to the number of blocks (time) it was stored.
        /// Reference <https://spec.filecoin.io/#section-systems.filecoin_markets.onchain_storage_market.storage_deal_states>
        storage_price_per_block: Balance,

        /// Amount of Balance (DOTs) Storage Provider stakes as Collateral for storing given `piece_cid`
        /// There should be enough Balance added by `add_balance` by Storage Provider to cover it.
        /// When the Deal fails/is terminated to early, this is the amount which get slashed.
        provider_collateral: Balance,
        /// Current [`DealState`].
        /// It goes: `Unpublished` -> `Published` -> `Active`
        state: DealState<BlockNumber>,
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
            let mh_bytes = bs58::decode(&self.piece_cid)
                .into_vec()
                .map_err(|e| ProposalError::Base58Error(e))?;
            let cid = Cid::new_v1(
                CID_CODEC,
                Multihash::from_bytes(&mh_bytes).map_err(|e| ProposalError::InvalidMultihash(e))?,
            );

            Ok(cid)
        }
    }

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
        DealPublished {
            deal_id: DealId,
            client: T::AccountId,
            provider: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// When a Market Participant tries to withdraw more
        /// funds than they have available on the Market, because:
        /// - they never deposited the amount they want to withdraw
        /// - the funds they deposited were locked as part of a deal
        InsufficientFreeFunds,
        NoProposalsToBePublished,
        ProposalsNotPublishedByStorageProvider,
        AllProposalsInvalid,
        NoValidProposals,
        WrongSignature,
    }

    // NOTE(@th7nder,18/06/2024):
    // would love to use `thiserror` but it's not supporting no_std environments yet
    // `thiserror-core` relies on rust nightly feature: error_in_core
    #[derive(RuntimeDebug)]
    pub enum ProposalError {
        WrongSignature,
        EndBeforeStart,
        NotUnpublished,
        DurationOutOfBounds,
        Base58Error(bs58::decode::Error),
        InvalidMultihash(cid::multihash::Error),
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
            // TODO(@th7nder,19/06/2024):
            // - pending proposal dedpulication
            // - struct DealId(u64)
            // - unit tests
            // - docs
            // - testing on substrate
            let provider = ensure_signed(origin)?;
            let (valid_deals, total_provider_lockup) =
                Self::validate_deals(provider.clone(), deals)?;

            // Lock up funds for the clients and emit events
            for mut deal in valid_deals.into_iter() {
                let client_fee: BalanceOf<T> = deal
                    .total_storage_fee()
                    .expect("should have been validated by now in validate_deals")
                    .try_into()
                    .ok()
                    .expect("should have been validated by now in validate_deals");

                BalanceTable::<T>::try_mutate(&deal.client, |balance| -> DispatchResult {
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
                &provider,
            )?;

            // Ensure the Piece's Cid is parsable and valid
            let _ = deal.proposal.cid()?;

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

            // TODO(@th7nder,17/06/2024): validate a Storage Provider's Account (whether the account was registerd as Storage Provider)
            // maybe call the StorageProviderPallet via loose coupling?

            let mut total_client_lockup: BoundedBTreeMap<T::AccountId, BalanceOf<T>, T::MaxDeals> =
                BoundedBTreeMap::new();
            let mut total_provider_lockup: BalanceOf<T> = Default::default();
            let mut message_proposals: BoundedBTreeSet<T::Hash, T::MaxDeals> =
                BoundedBTreeSet::new();

            let valid_deals = deals.into_iter().filter_map(|deal| {
                    if let Err(e) = Self::sanity_check(&deal, &provider) {
                        log::info!(target: LOG_TARGET, "insane deal: {:?}, error: {:?}", deal, e);
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
                        log::info!(target: LOG_TARGET, "invalid deal: client {:?} not enough free balance {:?} < {:?} to cover deal {:?}", 
                            deal.proposal.client, client_balance.free, client_lockup, deal);
                        return None;
                    }

                    let mut provider_lockup = total_provider_lockup;
                    provider_lockup = provider_lockup.checked_add(&deal.proposal.provider_collateral)?;

                    let provider_balance = BalanceTable::<T>::get(&deal.proposal.provider);
                    if provider_lockup > provider_balance.free {
                        log::info!(target: LOG_TARGET, "invalid deal: storage provider {:?} not enough free balance {:?} < {:?} to cover deal {:?}",
                            deal.proposal.provider, provider_balance.free, provider_lockup, deal);
                        return None;
                    }

                    let hash = Self::hash_proposal(&deal);
                    let duplicate_in_state = PendingProposals::<T>::get().contains(&hash);
                    let duplicate_in_message = message_proposals.contains(&hash);
                    if duplicate_in_state || duplicate_in_message {
                        log::info!(target: LOG_TARGET, "invalid deal: cannot publish duplicate deal: {:?}", deal);
                        return None;
                    }
                    PendingProposals::<T>::get().try_insert(hash.clone()).ok()?;
                    message_proposals.try_insert(hash).ok()?;

                    // SAFETY: it'll always succeed, as there cannot be more clients than T::MaxDeals
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
        // It is not an associated function, because T::Hashing was hard to use inside of there.
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

        /// Validates the signature of the given data with the provided signer's account ID.
        ///
        /// # Errors
        ///
        /// This function returns a [`WrongSignature`](crate::Error::WrongSignature) error if the
        /// signature is invalid or the verification process fails.
        pub fn validate_signature(
            data: &Vec<u8>,
            signature: &T::OffchainSignature,
            signer: &T::AccountId,
        ) -> Result<(), ProposalError> {
            if signature.verify(&**data, &signer) {
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
    }
}
