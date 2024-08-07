use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet};
use primitives_proofs::{SectorNumber, MAX_TERMINATIONS_PER_CALL};

use crate::{pallet::DECLARATIONS_MAX, partition::PartitionNumber};

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct FaultDeclaration {
    /// The deadline to which the faulty sectors are assigned, in range [0..WPoStPeriodDeadlines)
    pub deadline: u64,
    /// Partition index within the deadline containing the faulty sectors.
    pub partition: PartitionNumber,
    /// Sectors in the partition being declared faulty.
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
}

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct DeclareFaultsParams {
    pub faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
}
