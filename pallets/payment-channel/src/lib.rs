#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

// TODO(@Serhii, no-ref, 20/06/2024): when ready for prod - replace with #[frame_support::pallet]
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, ReservableCurrency},
    };
    use frame_system::pallet_prelude::*;
    use scale_info::TypeInfo;
    use sp_runtime::traits::{Hash, SaturatedConversion, Verify, Zero};
    use sp_runtime::{AccountId32, MultiSignature};

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    type ChainEpoch = u64;

    const SETTLE_DELAY: ChainEpoch = 12 * 60 * 60;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The currency mechanism.
        type Currency: ReservableCurrency<<Self as frame_system::Config>::AccountId>;
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AccountId: Parameter
            + Member
            + MaybeSerializeDeserialize
            + TypeInfo
            + Ord
            + PartialEq
            + Clone
            + Debug
            + Default
            + Into<AccountId32>;
    }

    /// Enumeration to represent the status of a payment channel.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub enum ChannelStatus {
        /// The channel is open and can be used for transactions.
        Open,
        /// The channel is in the process of settling.
        Settling,
        /// The channel has been closed and funds have been distributed.
        Closed,
    }

    /// Struct representing the state of the payment channel.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct ChannelState<AccountId, Balance> {
        /// The owner of the channel.
        pub from: AccountId,
        /// The recipient of payouts from the channel.
        pub to: AccountId,
        /// Amount successfully redeemed through the payment channel, paid out on `Collect`.
        pub to_send: Balance,
        /// Height at which the channel can be collected.
        pub settling_at: ChainEpoch,
        /// Height before which the channel `ToSend` cannot be collected.
        pub min_settle_height: ChainEpoch,
        /// Collection of lane states for the channel.
        pub lane_states: Vec<LaneState<Balance>>,
    }

    /// Struct representing a signed voucher.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct SignedVoucher<AccountId, Balance> {
        /// The address of the payment channel this signed voucher is valid for.
        pub channel_addr: AccountId,
        /// Min epoch before which the voucher cannot be redeemed.
        pub time_lock_min: ChainEpoch,
        /// Maximum time lock.
        pub time_lock_max: ChainEpoch,
        /// (optional) Used by `to` to validate.
        pub secret_pre_image: Vec<u8>,
        /// (optional) Specified by `from` to add a verification method to the voucher.
        pub extra: Option<ModVerifyParams<AccountId>>,
        /// Specifies which lane the Voucher merges into (will be created if does not exist).
        pub lane: u64,
        /// Set by `from` to prevent redemption of stale vouchers on a lane.
        pub nonce: u64,
        /// Amount to be paid.
        pub amount: Balance,
        /// (optional) Can extend channel min_settle_height if needed
        pub min_settle_height: ChainEpoch,
        /// (optional) Set of lanes to be merged into `lane`.
        pub merges: Vec<Merge>,
        /// Sender's signature over the voucher (sign on none).
        pub signature: Option<Vec<u8>>,
    }

    impl<AccountId: Encode, Balance: Encode> SignedVoucher<AccountId, Balance> {
        pub fn signing_bytes(&self) -> Vec<u8> {
            /// Helper struct to avoid cloning for serializing structure.
            #[derive(Encode)]
            struct SignedVoucherSer<'a, AccountId, Balance> {
                pub channel_addr: &'a AccountId,
                pub time_lock_min: u64,
                pub time_lock_max: u64,
                pub secret_pre_image: &'a [u8],
                pub extra: &'a Option<ModVerifyParams<AccountId>>,
                pub lane: u64,
                pub nonce: u64,
                pub amount: &'a Balance,
                pub min_settle_height: u64,
                pub merges: &'a [Merge],
                pub signature: (),
            }

            let osv = SignedVoucherSer {
                channel_addr: &self.channel_addr,
                time_lock_min: self.time_lock_min,
                time_lock_max: self.time_lock_max,
                secret_pre_image: &self.secret_pre_image,
                extra: &self.extra,
                lane: self.lane,
                nonce: self.nonce,
                amount: &self.amount,
                min_settle_height: self.min_settle_height,
                merges: &self.merges,
                signature: (),
            };

            // SCALE encode the struct
            osv.encode()
        }
    }

    /// Structure to represent the parameters for updating the channel state.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct UpdateChannelStateParams<AccountId, Balance> {
        /// The signed voucher.
        pub sv: SignedVoucher<AccountId, Balance>,
        /// The secret for the voucher.
        pub secret: Vec<u8>,
    }

    /// Structure to represent the additional parameters for verification.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct ModVerifyParams<AccountId> {
        /// The account involved in the verification process.
        pub verifier: AccountId,
        /// The method used for verification.
        pub method: u64,
        /// The data for verification.
        pub data: Vec<u8>,
    }

    /// Structure to represent the state of a lane.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct LaneState<Balance> {
        /// The amount redeemed in the lane.
        pub redeemed: Balance,
        /// The nonce of the lane.
        pub nonce: u64,
    }

    /// Structure to represent a merge operation within a signed voucher.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct Merge {
        /// The lane to be merged.
        pub lane: u64,
        /// The nonce of the lane to be merged.
        pub nonce: u64,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Storage to keep track of all payment channels.
    #[pallet::storage]
    pub type Channels<T: Config> = StorageMap<
        _,
        Twox64Concat,
        T::Hash,
        ChannelState<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
    >;

    /// Storage to keep track of lane states for each channel.
    #[pallet::storage]
    pub type LaneStates<T: Config> =
        StorageMap<_, Twox64Concat, T::Hash, Vec<LaneState<BalanceOf<T>>>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Event emitted when a new channel is opened.
        ChannelOpened { channel_id: T::Hash },
        /// Event emitted when a channel state is updated.
        ChannelUpdated { channel_id: T::Hash },
        /// Event emitted when a channel is settled.
        ChannelSettled { channel_id: T::Hash },
        /// Event emitted when a channel is collected.
        ChannelCollected { channel_id: T::Hash },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Invalid channel identifier.
        InvalidChannel,
        /// Invalid signature.
        InvalidSignature,
        /// Invalid secret.
        InvalidSecret,
        /// Insufficient balance in the channel.
        InsufficientBalance,
        /// The nonce of the voucher is outdated.
        InvalidNonce,
        /// The channel is already in the process of settling.
        ChannelAlreadySettling,
        /// The channel is not yet in the process of settling.
        ChannelNotSettling,
        /// Voucher has expired.
        VoucherExpired,
        /// Cannot use this voucher yet.
        VoucherNotYetValid,
        /// Voucher would leave channel balance negative.
        NegativeBalance,
        /// Failed to store lane state.
        LaneStateStorageFailed,
        /// Failed to save lanes.
        LaneStateSaveFailed,
        /// Maximum lane ID exceeded.
        MaxLaneExceeded,
        /// Unable to merge lane collections.
        InvalidMerge,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        AccountId32: From<<T as frame_system::Config>::AccountId>,
    {
        /// Opens a new payment channel.
        ///
        /// - `to`: The recipient account.
        /// - `initial_deposit`: The initial deposit to reserve for the channel.
        #[pallet::call_index(0)]
        // TODO(@Serhii, no-ref, 21/06/2024): implement benchmarks and generate weights
        #[pallet::weight(10_000)]
        pub fn open_channel(
            origin: OriginFor<T>,
            to: <T as frame_system::Config>::AccountId,
            initial_deposit: BalanceOf<T>,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;

            let channel_id = T::Hashing::hash_of(&(
                from.clone(),
                to.clone(),
                frame_system::Pallet::<T>::block_number(),
            ));

            T::Currency::reserve(&from, initial_deposit)?;

            let state = ChannelState {
                from: from.clone(),
                to: to.clone(),
                to_send: BalanceOf::<T>::zero(),
                settling_at: Zero::zero(),
                min_settle_height: Zero::zero(),
                lane_states: Default::default(),
            };

            Channels::<T>::insert(channel_id, state);

            Self::deposit_event(Event::ChannelOpened { channel_id });

            Ok(())
        }
        /// Updates the state of a payment channel.
        ///
        /// - `channel_id`: The unique identifier of the channel.
        /// - `params`: The parameters for updating the channel state.
        #[pallet::call_index(1)]
        // TODO: Implement benchmarks and generate weights
        #[pallet::weight(10_000)]
        pub fn update_channel_state(
            origin: OriginFor<T>,
            channel_id: T::Hash,
            params: UpdateChannelStateParams<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            Channels::<T>::try_mutate(channel_id, |channel| -> DispatchResult {
                let channel = channel.as_mut().ok_or(Error::<T>::InvalidChannel)?;
                ensure!(
                    caller == channel.from || caller == channel.to,
                    Error::<T>::InvalidChannel
                );

                let signer = if caller == channel.from {
                    &channel.to
                } else {
                    &channel.from
                };
                let sv = &params.sv;

                // Validate signature
                ensure!(
                    Self::verify_signature(sv, signer),
                    Error::<T>::InvalidSignature
                );

                let current_epoch =
                    <frame_system::Pallet<T>>::block_number().saturated_into::<ChainEpoch>();

                ensure!(
                    channel.settling_at == ChainEpoch::zero()
                        || current_epoch < channel.settling_at,
                    Error::<T>::ChannelAlreadySettling
                );
                ensure!(params.secret.len() > 256, Error::<T>::InvalidSecret);

                // Validate voucher constraints
                ensure!(
                    current_epoch >= sv.time_lock_min,
                    Error::<T>::VoucherNotYetValid
                );
                ensure!(
                    sv.time_lock_max == 0 || current_epoch <= sv.time_lock_max,
                    Error::<T>::VoucherExpired
                );
                ensure!(sv.amount >= Zero::zero(), Error::<T>::NegativeBalance);

                if !sv.secret_pre_image.is_empty() {
                    let hashed_secret: Vec<u8> =
                        sp_io::hashing::blake2_256(&params.secret).to_vec();
                    ensure!(
                        hashed_secret == sv.secret_pre_image,
                        Error::<T>::InvalidSecret
                    );
                }

                // Handle merging lanes and updating lane states
                LaneStates::<T>::try_mutate(channel_id, |lane_states| -> DispatchResult {
                    let lane_states = lane_states.as_mut().ok_or(Error::<T>::InvalidChannel)?;

                    // Find or create the lane state index
                    let lane_state_index = lane_states
                        .iter()
                        .position(|ls| ls.nonce == sv.lane)
                        .unwrap_or_else(|| {
                            lane_states.push(LaneState {
                                redeemed: BalanceOf::<T>::zero(),
                                nonce: sv.lane,
                            });
                            lane_states.len() - 1
                        });

                    let mut redeemed_from_others = BalanceOf::<T>::zero();
                    let merge_indices: Result<Vec<usize>, DispatchError> = sv
                        .merges
                        .iter()
                        .map(|merge| {
                            ensure!(merge.lane != sv.lane, Error::<T>::InvalidMerge);
                            lane_states
                                .iter()
                                .position(|ls| ls.nonce == merge.lane)
                                .ok_or(Error::<T>::InvalidMerge)
                                .map_err(Into::into)
                        })
                        .collect();

                    let merge_indices = merge_indices?;

                    for other_index in merge_indices {
                        let other_ls = &mut lane_states[other_index];
                        ensure!(other_ls.nonce <= sv.nonce, Error::<T>::InvalidNonce);

                        redeemed_from_others += other_ls.redeemed;
                        other_ls.nonce = sv.nonce;
                    }

                    // Prevent double counting by removing already redeemed amounts
                    let lane_state = &mut lane_states[lane_state_index];
                    lane_state.nonce = sv.nonce;
                    let balance_delta = sv.amount - (redeemed_from_others + lane_state.redeemed);

                    lane_state.redeemed = sv.amount;

                    // Check new send balance
                    let new_send_balance = balance_delta + channel.to_send;
                    ensure!(
                        new_send_balance >= Zero::zero(),
                        Error::<T>::NegativeBalance
                    );
                    ensure!(
                        new_send_balance <= T::Currency::free_balance(&channel.from),
                        Error::<T>::InsufficientBalance
                    );

                    channel.to_send = new_send_balance;

                    // Update settling_at and min_settle_height if delayed by voucher
                    if sv.min_settle_height != ChainEpoch::zero() {
                        if channel.settling_at != ChainEpoch::zero()
                            && channel.settling_at < sv.min_settle_height
                        {
                            channel.settling_at = sv.min_settle_height;
                        }
                        if channel.min_settle_height < sv.min_settle_height {
                            channel.min_settle_height = sv.min_settle_height;
                        }
                    }

                    Ok(())
                })?;

                Self::deposit_event(Event::ChannelUpdated { channel_id });

                Ok(())
            })
        }

        /// Initiates the settlement process for a channel.
        ///
        /// - `channel_id`: The unique identifier of the channel.
        #[pallet::call_index(2)]
        // TODO(@Serhii, no-ref, 21/06/2024): implement benchmarks and generate weights
        #[pallet::weight(10_000)]
        pub fn settle_channel(origin: OriginFor<T>, channel_id: T::Hash) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            Channels::<T>::try_mutate(channel_id, |channel| -> DispatchResult {
                let channel = channel.as_mut().ok_or(Error::<T>::InvalidChannel)?;
                ensure!(
                    caller == channel.from || caller == channel.to,
                    Error::<T>::InvalidChannel
                );

                // Check if the channel is already settling
                ensure!(
                    channel.settling_at == ChainEpoch::zero(),
                    Error::<T>::ChannelAlreadySettling
                );

                // Set the settling epoch and ensure it respects the minimum settle height
                let current_epoch =
                    <frame_system::Pallet<T>>::block_number().saturated_into::<ChainEpoch>();
                channel.settling_at = current_epoch + SETTLE_DELAY;
                if channel.settling_at < channel.min_settle_height {
                    channel.settling_at = channel.min_settle_height;
                }

                // Emit an event
                Self::deposit_event(Event::ChannelSettled { channel_id });

                Ok(())
            })
        }

        /// Finalizes the channel, distributing the remaining funds.
        ///
        /// - `channel_id`: The unique identifier of the channel.
        #[pallet::call_index(3)]
        // TODO(@Serhii, no-ref, 21/06/2024): implement benchmarks and generate weights
        #[pallet::weight(10_000)]
        pub fn collect_channel(origin: OriginFor<T>, channel_id: T::Hash) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            Channels::<T>::try_mutate_exists(channel_id, |channel| -> DispatchResult {
                let channel = channel.take().ok_or(Error::<T>::InvalidChannel)?;
                ensure!(
                    caller == channel.from || caller == channel.to,
                    Error::<T>::InvalidChannel
                );

                let current_epoch =
                    <frame_system::Pallet<T>>::block_number().saturated_into::<ChainEpoch>();
                ensure!(
                    channel.settling_at != ChainEpoch::zero()
                        && current_epoch >= channel.settling_at,
                    Error::<T>::ChannelNotSettling
                );

                // Transfer the `to_send` amount to the recipient
                T::Currency::unreserve(&channel.from, channel.to_send);
                T::Currency::transfer(
                    &channel.from,
                    &channel.to,
                    channel.to_send,
                    ExistenceRequirement::AllowDeath,
                )?;

                // Unreserve remaining balance in the "from" address
                let remaining_balance = T::Currency::reserved_balance(&channel.from);
                T::Currency::unreserve(&channel.from, remaining_balance);

                // Remove the channel state from storage
                Channels::<T>::remove(channel_id);

                // Emit an event
                Self::deposit_event(Event::ChannelCollected { channel_id });

                Ok(())
            })
        }
    }

    impl<T: Config> Pallet<T> {
        fn verify_signature(
            sv: &SignedVoucher<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
            signer: &<T as frame_system::Config>::AccountId,
        ) -> bool
        where
            <T as frame_system::Config>::AccountId: Into<AccountId32> + Clone,
        {
            // Convert AccountId to AccountId32
            let signer32: AccountId32 = signer.clone().into();

            // Get the serialized version of the SignedVoucher excluding the signature itself
            let signing_bytes = sv.signing_bytes();

            // Ensure the voucher contains a valid signature
            if let Some(signature) = &sv.signature {
                // Deserialize the signature into MultiSignature
                if let Ok(multi_sig) = MultiSignature::decode(&mut &signature[..]) {
                    // Verify the signature using the signer's public key
                    return multi_sig.verify(&signing_bytes[..], &signer32);
                }
            }

            false
        }
    }
}
