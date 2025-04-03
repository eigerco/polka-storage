use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Currency;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_arithmetic::traits::BaseArithmetic;
use sp_runtime::{traits::ConstU32, BoundedVec, RuntimeDebug};

use crate::{
    commitment::{CommP, Commitment, CommitmentError},
    configs::CurrencyProvider,
    deals::deal_state::DealState,
    CID_SIZE_IN_BYTES, MAX_LABEL_SIZE,
};

pub type DealProposalOf<T> = DealProposal<
    <T as frame_system::Config>::AccountId,
    <<T as CurrencyProvider>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance,
    BlockNumberFor<T>,
>;

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/deal.rs#L93>
// It cannot be generic over <T: Config> because, #[derive(RuntimeDebug, TypeInfo)] also make `T` to have `RuntimeDebug`/`TypeInfo`
// It is a known rust issue <https://substrate.stackexchange.com/questions/452/t-doesnt-implement-stdfmtdebug>
pub struct DealProposal<Address, Balance, BlockNumber> {
    /// Byte Encoded Cid
    // We use BoundedVec here, as cid::Cid do not implement `TypeInfo`, so it cannot be saved into the Runtime Storage.
    // It maybe doable using newtype pattern, however not sure how the UI on the frontend side would handle that anyways.
    // There is Encode/Decode implementation though, through the feature flag: `scale-codec`.
    pub piece_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
    /// The value represents the size of the data piece after padding to the
    /// nearest power of two. Padding ensures that all pieces can be
    /// efficiently arranged in a binary tree structure for Merkle proofs.
    pub piece_size: u64,
    /// Storage Client's Account Id
    pub client: Address,
    /// Storage Provider's Account Id
    pub provider: Address,

    /// Arbitrary client chosen label to apply to the deal
    pub label: BoundedVec<u8, ConstU32<MAX_LABEL_SIZE>>,

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

impl<Address, Balance, BlockNumber> DealProposal<Address, Balance, BlockNumber>
where
    Balance: BaseArithmetic + Copy,
    BlockNumber: BaseArithmetic + Copy,
{
    pub fn duration(&self) -> BlockNumber {
        self.end_block - self.start_block
    }

    pub fn total_storage_fee(&self) -> Option<u128> {
        // We need to convert into something to perform the calculation.
        // Generics trickery prevents us from doing it in a nice way.
        // <https://stackoverflow.com/questions/56081117/how-do-you-convert-between-substrate-specific-types-and-rust-primitive-types>
        Some(
            TryInto::<u128>::try_into(self.storage_price_per_block).ok()?
                * TryInto::<u128>::try_into(self.duration()).ok()?,
        )
    }

    pub fn piece_commitment(&self) -> Result<Commitment<CommP>, CommitmentError> {
        let commitment = Commitment::from_cid_bytes(&self.piece_cid[..])?;
        Ok(commitment)
    }
}
