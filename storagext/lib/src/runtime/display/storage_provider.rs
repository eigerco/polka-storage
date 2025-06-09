use crate::{
    runtime::{
        runtime_types::{
            pallet_storage_provider::{
                balance::BalanceEntry,
                deal::{
                    parameters::{DealDurationBound, DealParameters},
                    SettledDealData,
                },
                fault,
            },
            polka_storage_runtime::Runtime,
            primitives::{deals::deal_state::DealState, sector::pre_commit},
        },
        storage_provider::{events, Event},
    },
    types::storage_provider::DealProposal,
    BlockNumber,
};

impl std::fmt::Display for DealState<BlockNumber> {
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
            "Deal Proposal {{ piece_cid: {}, piece_size: {}, provider: {}, client: {}, label: {}, start_block: {}, end_block: {}, storage_price_per_block: {}, state: {} }}",
            self.piece_cid, self.piece_size, self.provider, self.client, self.label, self.start_block, self.end_block, self.storage_price_per_block, self.state
        ))
    }
}

impl std::fmt::Display for SettledDealData<Runtime> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Settled Deal {{ deal_id: {}, provider_account: {}, client_account: {}, amount: {} }}",
            self.deal_id, self.provider, self.client, self.amount
        ))
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

impl<Balance, BlockNumber> std::fmt::Display for DealParameters<Balance, BlockNumber>
where
    Balance: std::fmt::Display,
    BlockNumber: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "DealParameters {{ minimum_price_per_block: {}, deal_duration: {} }}",
            self.minimum_price_per_block, self.deal_duration,
        ))
    }
}

impl<BlockNumber> std::fmt::Display for DealDurationBound<BlockNumber>
where
    BlockNumber: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "DealDurationBound {{ lower: {}, upper: {} }}",
            self.lower, self.upper
        ))
    }
}

impl std::fmt::Display for fault::FaultDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Fault Declaration: {{ deadline: {}, partition: {}, sectors: [{}] }}",
            self.deadline,
            self.partition,
            itertools::Itertools::intersperse(
                self.sectors.0.iter().map(|sector| format!("{}", sector)),
                ", ".to_string()
            )
            .collect::<String>()
        ))
    }
}

impl std::fmt::Display for fault::RecoveryDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Recovery Declaration: {{ deadline: {}, partition: {}, sectors: [{}] }}",
            self.deadline,
            self.partition,
            itertools::Itertools::intersperse(
                self.sectors.0.iter().map(|sector| format!("{}", sector)),
                ", ".to_string()
            )
            .collect::<String>()
        ))
    }
}

impl<T> std::fmt::Display for pre_commit::SectorPreCommitInfo<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Sector Pre-Commit Info: {{ sector_number: {}, expiration: {}, seal_proof: {:?}, unsealed_cid: {}, sealed_cid: {} }}",
            self.sector_number,
            self.expiration,
            self.seal_proof,
            cid::Cid::read_bytes(self.unsealed_cid.0.as_slice()).expect("received corrupted CID"),
            cid::Cid::read_bytes(self.sealed_cid.0.as_slice()).expect("received corrupted CID"),
        ))
    }
}

// This type is a generated specialization of a more generic type,
// using this one is easier for Display rather than coping with ultra-generic bounds
impl std::fmt::Display for events::storage_provider_registered::Info {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Storage Provider Info: {{ peer_id: {}, window_post_proof_type: {:?}, sector_size: {:?}, window_post_partition_sectors: {} }}",
            // This matches the libp2p implementation without requiring such a big dependency
            bs58::encode(self.peer_id.0.as_slice()).into_string(),
            self.window_post_proof_type,
            self.sector_size,
            self.window_post_partition_sectors,
        ))
    }
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::StorageProviderRegistered {
                owner,
                info,
                proving_period_start,
            } => f.write_fmt(format_args!(
                "Storage Provider Registered: {{ owner: {}, info: {}, proving_period_start: {} }}",
                owner, info, proving_period_start,
            )),
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
            Event::DealParametersUpdated { provider, deal_parameters } => f.write_fmt(format_args!(
                "Deal Parameters Updated: {{ provider: {}, deal_parameters: {} }}",
                provider, deal_parameters
            )),
            Event::DealParametersRemoved { provider } => f.write_fmt(format_args!(
                "Deal Parameters Removed: {{ provider: {} }}",
                provider
            )),
            Event::SectorsPreCommitted {
                block,
                owner,
                sectors,
            } => f.write_fmt(format_args!(
                "Sectors Pre-Committed: {{ block: {}, owner: {}, sector_number: {:?} }}",
                block, owner, sectors,
            )),
            Event::SectorsProven { owner, sectors } => f.write_fmt(format_args!(
                "Sectors Proven: {{ owner: {}, sectors: {:?} }}",
                owner, sectors,
            )),
            Event::SectorsSlashed {
                owner,
                sector_numbers,
            } => f.write_fmt(format_args!(
                "Sectors Slashed: {{ owner: {}, sector_numbers: {} }}",
                owner,
                itertools::Itertools::intersperse(
                    sector_numbers.0.iter().map(ToString::to_string),
                    ", ".to_string()
                )
                .collect::<String>(),
            )),
            Event::ValidPoStSubmitted { owner } => {
                f.write_fmt(format_args!("Valid PoSt Submitted: {{ owner: {} }}", owner,))
            }
            Event::FaultsDeclared { owner, faults } => f.write_fmt(format_args!(
                "Faults Declared: {{ owner: {}, faults: [{}] }}",
                owner,
                itertools::Itertools::intersperse(
                    faults.0.iter().map(|fault| format!("{}", fault)),
                    ", ".to_string()
                )
                .collect::<String>()
            )),
            Event::FaultsRecovered { owner, recoveries } => f.write_fmt(format_args!(
                "Faults Recovered: {{ owner: {}, recoveries: [{}] }}",
                owner,
                itertools::Itertools::intersperse(
                    recoveries.0.iter().map(|recovery| format!("{}", recovery)),
                    ", ".to_string()
                )
                .collect::<String>()
            )),
            Event::PartitionsFaulty {
                owner,
                faulty_partitions,
            } => f.write_fmt(format_args!(
                "Faulty Partitions: {{ owner: {}, faulty_partitions: [{}] }}",
                owner,
                itertools::Itertools::intersperse(
                    faulty_partitions
                        .0
                        .iter()
                        .map(|(partition, sectors)| format!(
                            "{{ partition: {}, sectors: {} }}",
                            partition,
                            itertools::Itertools::intersperse(
                                sectors.0.iter().map(ToString::to_string),
                                ", ".to_string()
                            )
                            .collect::<String>()
                        )),
                    ", ".to_string()
                )
                .collect::<String>()
            )),
            Event::SectorsTerminated {
                owner,
                terminations,
            } => f.write_fmt(format_args!(
                "Sectors terminated: {{ owner: {}, terminations: [{}]",
                owner,
                itertools::Itertools::intersperse(
                    terminations
                        .0
                        .iter()
                        .map(|termination| format!("{termination:?}")),
                    ", ".to_string()
                )
                .collect::<String>()
            )),
        }
    }
}
