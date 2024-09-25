//! Filecoin type definitions to make it no-std compatible.

use merkletree::store::StoreConfig;

pub type ProverId = [u8; 32];
pub type Commitment = [u8; 32];
pub type Ticket = [u8; 32];
pub type ReplicaId = Fr;

#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord)]
pub struct PaddedBytesAmount(pub u64);

#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord)]
pub struct UnpaddedBytesAmount(pub u64);

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PieceInfo {
    pub commitment: Commitment,
    pub size: UnpaddedBytesAmount,
}

impl Debug for PieceInfo {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("PieceInfo")
            .field("commitment", &hex::encode(self.commitment))
            .field("size", &self.size)
            .finish()
    }
}

impl PieceInfo {
    pub fn new(size: UnpaddedBytesAmount) -> Result<Self> {
        Ok(PieceInfo { commitment: [0u8; 32], size })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SectorSize(pub u64);

impl From<u64> for SectorSize {
    fn from(size: u64) -> Self {
        SectorSize(size)
    }
}

impl From<SectorSize> for UnpaddedBytesAmount {
    fn from(x: SectorSize) -> Self {
        UnpaddedBytesAmount(to_unpadded_bytes(x.0))
    }
}

impl From<SectorSize> for PaddedBytesAmount {
    fn from(x: SectorSize) -> Self {
        PaddedBytesAmount(x.0)
    }
}

impl From<SectorSize> for u64 {
    fn from(x: SectorSize) -> Self {
        x.0
    }
}

pub type Labels(Vec<StoreConfig>);

// TODO: Already defined in `pallet_proof::graphs::stacked`.
pub type BucketGraphSeed = [u8; 28];

pub struct BucketGraph {
    base_degree: usize,
    seed: BucketGraphSeed,
    nodes: usize,
}

pub struct StackedBucketGraph {
    base_graph: BucketGraph,
    feistel_keys: [feistel::Index; 4],
    feistel_precomputed: feistel::Precomputed,
}
