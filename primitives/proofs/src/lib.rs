#![cfg_attr(not(feature = "std"), no_std)]

pub mod randomness;
mod traits;
mod types;

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
pub use traits::*;
pub use types::*;

/// Derives a unique prover ID for a given account.
///
/// The function takes an `AccountId` and generates a 32-byte array that serves
/// as a unique identifier for the prover associated with that account. The
/// prover ID is derived using the Blake2 hash of the encoded account ID.
pub fn derive_prover_id<AccountId>(account_id: AccountId) -> [u8; 32]
where
    AccountId: Encode,
{
    let encoded = account_id.encode();
    let mut encoded = sp_core::blake2_256(&encoded);
    // Necessary to be valid bls12 381 element.
    encoded[31] &= 0x3f;
    encoded
}

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
