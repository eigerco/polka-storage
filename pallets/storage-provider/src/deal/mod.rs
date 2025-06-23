pub mod parameters;

use codec::{Decode, Encode};
use primitives::{self, DealId};
use scale_info::TypeInfo;

use crate::{BalanceOf, Config};

#[derive(TypeInfo, Encode, Decode, Clone, PartialEq)]
pub struct PublishedDeal<T: Config> {
    pub client: T::AccountId,
    pub deal_id: DealId,
}

impl<T> core::fmt::Debug for PublishedDeal<T>
where
    T: Config,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PublishedDeal")
            .field("deal_id", &self.deal_id)
            .field("client", &self.client)
            .finish()
    }
}

/// The data part of the event pushed when the deal is successfully settled.
#[derive(TypeInfo, Encode, Decode, Clone, PartialEq)]
pub struct SettledDealData<T: Config> {
    pub deal_id: DealId,
    pub client: T::AccountId,
    pub provider: T::AccountId,
    pub amount: BalanceOf<T>,
}

impl<T> core::fmt::Debug for SettledDealData<T>
where
    T: Config,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SettledDealData")
            .field("deal_id", &self.deal_id)
            .field("client", &self.client)
            .field("provider", &self.provider)
            .field("amount", &self.amount)
            .finish()
    }
}

// Clone and PartialEq required because of the BoundedVec<(DealId, DealSettlementError)>
#[derive(TypeInfo, Encode, Decode, Clone, PartialEq, thiserror::Error)]
pub enum DealSettlementError {
    /// The deal is going to be slashed.
    #[error("DealSettlementError: Slashed Deal")]
    SlashedDeal,
    /// The deal last update is in the future — i.e. `last_update_block > current_block`.
    #[error("DealSettlementError: Future Last Update")]
    FutureLastUpdate,
    /// The deal was not found.
    #[error("DealSettlementError: Deal Not Found")]
    DealNotFound,
    /// The deal is too early to settle.
    #[error("DealSettlementError: Early Settlement")]
    EarlySettlement,
    /// The deal has expired
    #[error("DealSettlementError: Expired Deal")]
    ExpiredDeal,
    /// Deal is not activated
    #[error("DealSettlementError: Deal Not Active")]
    DealNotActive,
}

impl core::fmt::Debug for DealSettlementError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}
