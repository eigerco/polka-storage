use crate::address::{BLS_PUB_LEN, MAX_SUBADDRESS_LEN, PAYLOAD_HASH_LEN};
use crate::ActorID;
use codec::{Decode, Encode};

/// A "delegated" (f4) address.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Decode, Encode)]
pub struct DelegatedAddress {
    namespace: ActorID,
    length: u64,
    buffer: [u8; MAX_SUBADDRESS_LEN],
}

/// Payload is the data of the Address. Variants are the supported Address protocols.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Decode, Encode)]
pub enum Payload {
    /// f0: ID protocol address.
    ID(u64),
    /// f1: SECP256K1 key address, 20 byte hash of PublicKey.
    Secp256k1([u8; PAYLOAD_HASH_LEN]),
    /// f2: Actor protocol address, 20 byte hash of actor data.
    Actor([u8; PAYLOAD_HASH_LEN]),
    /// f3: BLS key address, full 48 byte public key.
    BLS([u8; BLS_PUB_LEN]),
    /// f4: Delegated address, a namespace with an arbitrary subaddress.
    Delegated(DelegatedAddress),
}
