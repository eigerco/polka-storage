use crate::{
    runtime::{
        market::Event,
        runtime_types::{
            pallet_market::pallet::{self, BalanceEntry, DealState},
            polka_storage_runtime::Runtime,
        },
    },
    types::market::DealProposal,
};

impl std::fmt::Display for DealState<u64> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DealState::Published => f.write_str("Published"),
            DealState::Active(state) => f.write_fmt(format_args!("Active({{ sector_number: {}, sector_start_block: {}, last_updated_block: {:?}, slash_block: {:?} }})", state.sector_number, state.sector_start_block, state.last_updated_block, state.slash_block)),
        }
    }
}

impl std::fmt::Display for DealProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Deal Proposal {{ piece_cid: {}, piece_size: {}, provider: {}, client: {}, label: {}, start_block: {}, end_block: {}, storage_price_per_block: {}, provider_collateral: {}, state: {} }}",
            self.piece_cid, self.piece_size, self.provider, self.client, self.label, self.start_block, self.end_block, self.storage_price_per_block, self.provider_collateral, self.state
        ))
    }
}

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
            Event::DealsPublished {
                provider,
                deals
            } => {
                // This should show something like
                // Deals Published: {
                //     provider_account: ...,
                //     deals: [
                //         { client_account: ..., deal_id: ... },
                //         { client_account: ..., deal_id: ... },
                //     ]
                // }
                f.write_fmt(format_args!(
                    "Deal Published: {{\n    provider_account: {},\n    deals: [\n",
                  provider
                ))?;
                for deal in deals.0.iter() {
                    f.write_fmt(format_args!("        {{ client_account: {}, deal_id: {} }},\n", deal.client, deal.deal_id))?;
                }
                f.write_str("    ]\n}")
            }
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

impl<T> std::fmt::Display for BalanceEntry<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Balance {{ free: {}, locked: {} }}",
            self.free, self.locked
        ))
    }
}
