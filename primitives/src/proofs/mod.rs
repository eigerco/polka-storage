mod traits;
mod types;

use codec::Encode;
use sp_core::blake2_256;
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
    let mut encoded = blake2_256(&encoded);

    // Necessary to be a valid bls12 381 element.
    encoded[31] &= 0x3f;
    encoded
}
