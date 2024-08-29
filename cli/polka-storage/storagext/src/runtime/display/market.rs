use crate::runtime::market::events;

impl std::fmt::Display for events::BalanceAdded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ account: {}, amount: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.who,
            self.amount
        ))
    }
}

impl std::fmt::Display for events::BalanceWithdrawn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // self.who is subxt::ext::subxt_core::utils::AccountId32 which has a private to_ss58check
        // we need to convert it to get the proper to_ss58check
        f.write_fmt(format_args!(
            "{} {{ account: {}, amount: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.who,
            self.amount
        ))
    }
}

impl std::fmt::Display for events::DealActivated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ dead_id: {}, provider_account: {}, client_account: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.deal_id,
            self.provider,
            self.client,
        ))
    }
}

impl std::fmt::Display for events::DealPublished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ dead_id: {}, provider_account: {}, client_account: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.deal_id,
            self.provider,
            self.client,
        ))
    }
}

impl std::fmt::Display for events::DealSlashed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ deal_id: {}, amount_slashed: {}, provider_account: {}, client_account: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.deal_id,
            self.amount,
            self.provider,
            self.client
        ))
    }
}

impl std::fmt::Display for events::DealsSettled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // we need to use intersperse like this because the compiler thinks we're using the nightly API
        // https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.intersperse
        // https://github.com/rust-lang/rust/issues/89151#issuecomment-2063584575
        let successful = itertools::Itertools::intersperse(
            self.successful.0.iter().map(|id| format!("{}", id)),
            ", ".to_string(),
        )
        .collect::<String>();
        let unsuccessful = itertools::Itertools::intersperse(
            self.unsuccessful
                .0
                .iter()
                // NOTE: the error may have a better formatting but for events::now, this is what we have
                .map(|(id, err)| format!("{{ id: {}, error: {:?} }}", id, err)),
            ", ".to_string(),
        )
        .collect::<String>();

        f.write_fmt(format_args!(
            "{} {{ successful: [{}], unsuccessful: [{}] }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            successful,
            unsuccessful
        ))
    }
}

impl std::fmt::Display for events::DealTerminated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ deal_id: {}, provider_account: {}, client_account: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.deal_id,
            self.provider,
            self.client
        ))
    }
}
