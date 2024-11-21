#![cfg_attr(not(feature = "std"), no_std)]

pub mod randomness;
mod traits;
mod types;

use codec::{Codec, Encode};
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

sp_api::decl_runtime_apis! {
    pub trait StorageProviderApi<AccountId> where AccountId: Codec
    {
        fn current_deadline(storage_provider: AccountId) -> u64;
    }
}