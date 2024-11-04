/// Byte representation of the entity that was signing the proof.
/// It must match the ProverId used for Proving.
pub type ProverId = [u8; 32];
/// Byte representation of a commitment - CommR or CommD.
pub type Commitment = [u8; 32];
/// Byte representation of randomness seed, it's used for challenge generation.
pub type Ticket = [u8; 32];
