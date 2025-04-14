use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

use crate::deals::active_deal_state::ActiveDealState;

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum DealState<BlockNumber> {
    /// Deal has been accepted on-chain by both Storage Provider and Storage Client, it's waiting for activation.
    Published,
    /// Deal has been activated
    Active(ActiveDealState<BlockNumber>),
}
