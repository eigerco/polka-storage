use primitives::PEER_ID_MAX_BYTES;
use sp_runtime::{traits::ConstU32, AccountId32, BoundedVec, MultiSigner};

#[derive(Debug)]
pub struct StorageProviderData {
    pub account_id: AccountId32,
    pub sign: MultiSigner,
    pub peer_id: BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>,
}
