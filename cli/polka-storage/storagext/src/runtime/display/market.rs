use crate::runtime::{
    market::Event,
    runtime_types::{pallet_market::pallet, polka_storage_runtime::Runtime},
};

impl std::fmt::Display for pallet::SettledDealData<Runtime> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Settled Deal {{ deal_id: {}, provider_account: {}, client_account: {}, amount: {} }}",
            self.deal_id, self.provider, self.client, self.amount
        ))
    }
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::BalanceAdded { who, amount } => f.write_fmt(
                format_args!("Balance Added: {{ account: {}, amount: {} }}", who, amount),
            ),
            Event::BalanceWithdrawn { who, amount } => {
                f.write_fmt(format_args!(
                    "Balance Withdrawn: {{ account: {}, amount: {} }}",
                    who, amount
                ))
            }
            Event::DealPublished {
                deal_id,
                client,
                provider,
            } => f.write_fmt(format_args!(
                "Deal Published: {{ deal_id: {}, provider_account: {}, client_account: {} }}",
                deal_id, provider, client,
            )),
            Event::DealActivated {
                deal_id,
                client,
                provider,
            } => f.write_fmt(format_args!(
                "Deal Activated: {{ deal_id: {}, provider_account: {}, client_account: {} }}",
                deal_id, provider, client,
            )),
            Event::DealsSettled {
                successful,
                unsuccessful,
            } => {
                // we need to use intersperse like this because the compiler thinks we're using the nightly API
                // https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.intersperse
                // https://github.com/rust-lang/rust/issues/89151#issuecomment-2063584575
                let successful = itertools::Itertools::intersperse(
                    successful.0.iter().map(|id| format!("{}", id)),
                    ", ".to_string(),
                )
                .collect::<String>();
                let unsuccessful = itertools::Itertools::intersperse(
                    unsuccessful
                        .0
                        .iter()
                        // NOTE: the error may have a better formatting but for events::now, this is what we have
                        .map(|(id, err)| format!("{{ id: {}, error: {:?} }}", id, err)),
                    ", ".to_string(),
                )
                .collect::<String>();

                f.write_fmt(format_args!(
                    "Deals Settled: {{ successful: [{}], unsuccessful: [{}] }}",
                    successful, unsuccessful
                ))
            }
            Event::DealSlashed {
                deal_id,
                amount,
                client,
                provider,
            } => f.write_fmt(format_args!(
                "Deal Slashed: {{ deal_id: {}, amount_slashed: {}, provider_account: {}, client_account: {} }}",
                deal_id,
                amount,
                provider,
                client
            )),
            Event::DealTerminated {
                deal_id,
                client,
                provider,
            } => f.write_fmt(format_args!(
                "Deal Terminated: {{ deal_id: {}, provider_account: {}, client_account: {} }}",
                deal_id, provider, client
            )),
        }
    }
}
