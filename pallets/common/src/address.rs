use codec::{Decode, Encode};
pub use payload::Payload;

mod payload;

/// Hash length of payload for SECP and Actor addresses.
pub const PAYLOAD_HASH_LEN: usize = 20;

/// BLS public key length used for validation of BLS addresses.
pub const BLS_PUB_LEN: usize = 48;

/// Max length of f4 sub addresses.
pub const MAX_SUBADDRESS_LEN: usize = 54;

/// Address is the struct that defines the protocol and data payload conversion from either
/// a public key or value
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct Address {
    payload: Payload,
}
