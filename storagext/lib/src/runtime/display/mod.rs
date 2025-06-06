mod faucet;
mod proofs;
mod storage_provider;

#[cfg(test)]
mod test {
    use crate::runtime::storage_provider;

    /// Call like `has_display::<TypeToBeChecked>()`,
    /// if `TypeToBeChecked` does not implement `Display`, compilation will fail
    fn has_traits<D: std::fmt::Display + serde::Serialize>() {}

    #[test]
    fn ensure_storage_provider_events_impl_display() {
        has_traits::<storage_provider::Event>();
    }
}
