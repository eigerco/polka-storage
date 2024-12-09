use frame_support::pallet_prelude::{Decode, Encode, RuntimeDebug, TypeInfo};
use frame_system::pallet_prelude::BlockNumberFor;

#[derive(PartialEq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub enum RequestType<T: frame_system::Config> {
    BabeEpoch(u64),
    Local(BlockNumberFor<T>),
}

#[derive(PartialEq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct RandomnessResult<Hash> {
    pub randomness: Option<Hash>,
    pub request_count: u64,
}
