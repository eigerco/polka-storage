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
        has_display::<market::Event>();
    }

    #[test]
    fn ensure_storage_provider_events_impl_display() {
        has_display::<storage_provider::Event>();
    }
}
