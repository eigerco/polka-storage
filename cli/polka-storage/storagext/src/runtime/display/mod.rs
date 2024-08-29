mod market;
mod storage_provider;

#[cfg(test)]
mod test {
    use crate::runtime::{market, storage_provider};

    /// Call like `has_display::<TypeToBeChecked>()`,
    /// if `TypeToBeChecked` does not implement `Display`, compilation will fail
    fn has_display<D: std::fmt::Display>() {}

    #[test]
    fn ensure_market_events_impl_display() {
        // Market
        has_display::<market::events::BalanceAdded>();
        has_display::<market::events::BalanceWithdrawn>();
        has_display::<market::events::DealActivated>();
        has_display::<market::events::DealPublished>();
        has_display::<market::events::DealTerminated>();
        has_display::<market::events::DealsSettled>();
    }

    #[test]
    fn ensure_storage_provider_events_impl_display() {
        // Storage Provider
        has_display::<storage_provider::events::FaultsDeclared>();
        has_display::<storage_provider::events::FaultsRecovered>();
        has_display::<storage_provider::events::PartitionFaulty>();
        has_display::<storage_provider::events::SectorPreCommitted>();
        has_display::<storage_provider::events::SectorProven>();
        has_display::<storage_provider::events::SectorSlashed>();
        has_display::<storage_provider::events::StorageProviderRegistered>();
        has_display::<storage_provider::events::ValidPoStSubmitted>();
    }
}
