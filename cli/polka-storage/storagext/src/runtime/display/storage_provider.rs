use crate::runtime::{
    runtime_types::pallet_storage_provider::{fault, sector},
    storage_provider::events,
};

impl std::fmt::Display for fault::FaultDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "FaultDeclaration {{ deadline: {}, partition: {}, sectors: [{}] }}",
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
            "RecoveryDeclaration {{ deadline: {}, partition: {}, sectors: [{}] }}",
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
            "SectorPreCommitInfo {{ sector_number: {}, expiration: {}, seal_proof: {:?}, unsealed_cid: {}, sealed_cid: {} }}",
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
            "StorageProviderInfo {{ peer_id: {}, window_post_proof_type: {:?}, sector_size: {:?}, window_post_partition_sectors: {} }}",
            // This matches the libp2p implementation without requiring such a big dependency
            bs58::encode(self.peer_id.0.as_slice()).into_string(),
            self.window_post_proof_type,
            self.sector_size,
            self.window_post_partition_sectors,
        ))
    }
}

impl std::fmt::Display for events::FaultsDeclared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, faults: [{}] }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            itertools::Itertools::intersperse(
                self.faults.0.iter().map(|fault| format!("{}", fault)),
                ", ".to_string()
            )
            .collect::<String>()
        ))
    }
}

impl std::fmt::Display for events::FaultsRecovered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, recoveries: [{}] }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            itertools::Itertools::intersperse(
                self.recoveries
                    .0
                    .iter()
                    .map(|recovery| format!("{}", recovery)),
                ", ".to_string()
            )
            .collect::<String>()
        ))
    }
}

impl std::fmt::Display for events::PartitionFaulty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, partition: {}, sectors: [{}] }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            self.partition,
            itertools::Itertools::intersperse(
                self.sectors
                    .0
                    .iter()
                    .map(|recovery| format!("{}", recovery)),
                ", ".to_string()
            )
            .collect::<String>()
        ))
    }
}

impl std::fmt::Display for events::SectorPreCommitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, sector_number: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            self.sector,
        ))
    }
}

impl std::fmt::Display for events::SectorProven {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, sector_number: {}, partition_number: {}, deadline_idx: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            self.sector_number,
            self.partition_number,
            self.deadline_idx,
        ))
    }
}

impl std::fmt::Display for events::SectorSlashed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, sector_number: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            self.sector_number,
        ))
    }
}

impl std::fmt::Display for events::StorageProviderRegistered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {}, info: {}, proving_period_start: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
            self.info,
            self.proving_period_start,
        ))
    }
}

impl std::fmt::Display for events::ValidPoStSubmitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {{ owner: {} }}",
            <Self as subxt::ext::subxt_core::events::StaticEvent>::EVENT,
            self.owner,
        ))
    }
}
