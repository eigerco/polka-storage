use codec::{Decode, Encode};
use scale_info::prelude::format;
use scale_info::prelude::string::String;
use scale_info::TypeInfo;

/// SectorNumber is a numeric identifier for a sector.
pub type SectorNumber = u64;

/// Content identifier
pub type Cid = String;

#[derive(Decode, Encode, TypeInfo)]
pub struct StorageProviderInfo<
    AccountId: Encode + Decode + Eq + PartialEq,
    PeerId: Encode + Decode + Eq + PartialEq,
    StoragePower: Encode + Decode + Eq + PartialEq,
> {
    /// The owner of this storage_provider.
    pub owner: AccountId,
    /// storage_provider's libp2p peer id in bytes.
    pub peer_id: PeerId,
    /// The total power the storage provider has
    pub total_raw_power: StoragePower,
    /// The price of storage (in DOT) for each block the storage provider takes for storage.
    // TODO(aidan46, no-ref, 2024-06-04): Use appropriate type
    pub price_per_block: String,
}

/// SectorSize indicates one of a set of possible sizes in the network.
#[repr(u64)]
pub enum SectorSize {
    _2KiB = 2_048,
    _8MiB = 8_388_608,
    _512MiB = 536_870_912,
    _32GiB = 34_359_738_368,
    _64GiB = 68_719_476_736,
}

/// Proof of spacetime type, indicating version and sector size of the proof.
#[derive(Decode, Encode, TypeInfo)]
pub enum RegisteredPoStProof {
    StackedDRGWinning2KiBV1,
    StackedDRGWinning8MiBV1,
    StackedDRGWinning512MiBV1,
    StackedDRGWinning32GiBV1,
    StackedDRGWinning64GiBV1,
    StackedDRGWindow2KiBV1P1,
    StackedDRGWindow8MiBV1P1,
    StackedDRGWindow512MiBV1P1,
    StackedDRGWindow32GiBV1P1,
    StackedDRGWindow64GiBV1P1,
    Invalid(i64),
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    #[allow(unused)]
    pub fn sector_size(self) -> Result<SectorSize, String> {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 | StackedDRGWinning2KiBV1 => Ok(SectorSize::_2KiB),
            StackedDRGWindow8MiBV1P1 | StackedDRGWinning8MiBV1 => Ok(SectorSize::_8MiB),
            StackedDRGWindow512MiBV1P1 | StackedDRGWinning512MiBV1 => Ok(SectorSize::_512MiB),
            StackedDRGWindow32GiBV1P1 | StackedDRGWinning32GiBV1 => Ok(SectorSize::_32GiB),
            StackedDRGWindow64GiBV1P1 | StackedDRGWinning64GiBV1 => Ok(SectorSize::_64GiB),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }

    /// Proof size for each PoStProof type
    #[allow(unused)]
    pub fn proof_size(self) -> Result<usize, String> {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWinning2KiBV1
            | StackedDRGWinning8MiBV1
            | StackedDRGWinning512MiBV1
            | StackedDRGWinning32GiBV1
            | StackedDRGWinning64GiBV1
            | StackedDRGWindow2KiBV1P1
            | StackedDRGWindow8MiBV1P1
            | StackedDRGWindow512MiBV1P1
            | StackedDRGWindow32GiBV1P1
            | StackedDRGWindow64GiBV1P1 => Ok(192),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    #[allow(unused)]
    pub fn window_post_partitions_sector(self) -> Result<u64, String> {
        // Resolve to post proof and then compute size from that.
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWinning64GiBV1 | StackedDRGWindow64GiBV1P1 => Ok(2300),
            StackedDRGWinning32GiBV1 | StackedDRGWindow32GiBV1P1 => Ok(2349),
            StackedDRGWinning2KiBV1 | StackedDRGWindow2KiBV1P1 => Ok(2),
            StackedDRGWinning8MiBV1 | StackedDRGWindow8MiBV1P1 => Ok(2),
            StackedDRGWinning512MiBV1 | StackedDRGWindow512MiBV1P1 => Ok(2),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
}

/// Seal proof type which defines the version and sector size.
#[allow(non_camel_case_types)]
#[derive(Decode, Encode, TypeInfo)]
pub enum RegisteredSealProof {
    StackedDRG2KiBV1,
    StackedDRG512MiBV1,
    StackedDRG8MiBV1,
    StackedDRG32GiBV1,
    StackedDRG64GiBV1,
    StackedDRG2KiBV1P1,
    StackedDRG512MiBV1P1,
    StackedDRG8MiBV1P1,
    StackedDRG32GiBV1P1,
    StackedDRG64GiBV1P1,
    StackedDRG2KiBV1P1_Feat_SyntheticPoRep,
    StackedDRG512MiBV1P1_Feat_SyntheticPoRep,
    StackedDRG8MiBV1P1_Feat_SyntheticPoRep,
    StackedDRG32GiBV1P1_Feat_SyntheticPoRep,
    StackedDRG64GiBV1P1_Feat_SyntheticPoRep,
    Invalid(i64),
}

/// This type is passed into the pre commit function on the storage provider pallet
#[derive(Decode, Encode, TypeInfo)]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub expiration: u64,
}
