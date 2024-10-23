use crate::runtime::{
    runtime_types::pallet_storage_provider::{fault, sector},
    storage_provider::{events, Event},
};

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

impl<T> std::fmt::Display for sector::SectorPreCommitInfo<T>
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
            Event::SectorsPreCommitted { owner, sectors } => f.write_fmt(format_args!(
                "Sectors Pre-Committed: {{ owner: {}, sector_number: {:?} }}",
                owner, sectors,
            )),
            Event::SectorsProven { owner, sectors } => f.write_fmt(format_args!(
                "Sectors Proven: {{ owner: {}, sectors: {:?} }}",
                owner, sectors,
            )),
            Event::SectorSlashed {
                owner,
                sector_number,
            } => f.write_fmt(format_args!(
                "Sector Slashed: {{ owner: {}, sector_number: {} }}",
                owner, sector_number,
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
            Event::PartitionFaulty {
                owner,
                partition,
                sectors,
            } => f.write_fmt(format_args!(
                "Faulty Partition: {{ owner: {}, partition: {}, sectors: [{}] }}",
                owner,
                partition,
                itertools::Itertools::intersperse(
                    sectors.0.iter().map(|recovery| format!("{}", recovery)),
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
