use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;

/// Current deadline in a proving period of a Storage Provider.
#[derive(Encode, Decode, TypeInfo)]
pub struct CurrentDeadline<BlockNumber> {
    /// Index of a deadline.
    ///
    /// If there are 10 deadlines if the proving period, values will be [0, 9].
    /// After proving period rolls over, it'll start from 0 again.
    pub deadline_index: u64,
    /// Whether the deadline is open.
    /// Only is false when `current_block < sp.proving_period_start`.
    pub open: bool,
    /// [`pallet_storage_provider::DeadlineInfo::challenge`].
    ///
    /// Block at which the randomness should be fetched to generate/verify Post.
    pub challenge_block: BlockNumber,
    /// Block at which the deadline opens.
    pub start: BlockNumber,
}

sp_api::decl_runtime_apis! {
    pub trait StorageProviderApi<AccountId> where AccountId: Codec
    {
        /// Gets the current deadline of the storage provider.
        ///
        /// If there is no Storage Provider of given AccountId returns [`Option::None`].
        /// May exceptionally return [`Option::None`] when
        /// conversion between BlockNumbers fails, but technically should not ever happen.
        fn current_deadline(storage_provider: AccountId) -> Option<
            CurrentDeadline<
                <<Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number
            >
        >;
    }
}
