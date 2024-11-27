use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet};
use primitives::{sector::SectorNumber, MAX_TERMINATIONS_PER_CALL};

use crate::{pallet::DECLARATIONS_MAX, partition::PartitionNumber};

/// Used by the storage provider to indicate a fault.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct FaultDeclaration {
    /// The deadline to which the faulty sectors are assigned, in range [0..WPoStPeriodDeadlines)
    pub deadline: u64,
    /// Partition index within the deadline containing the faulty sectors.
    pub partition: PartitionNumber,
    /// Sectors in the partition being declared faulty.
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
}

/// Type use as a parameter for `declare_faults` extrinsic.
/// Holds N amount of [`FaultDeclaration`] where N < DECLARATION_MAX.
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct DeclareFaultsParams {
    pub faults: BoundedVec<FaultDeclaration, ConstU32<DECLARATIONS_MAX>>,
}

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct RecoveryDeclaration {
    /// The deadline to which the recovered sectors are assigned, in range [0..WPoStPeriodDeadlines)
    pub deadline: u64,
    /// Partition index within the deadline containing the recovered sectors.
    pub partition: PartitionNumber,
    /// Sectors in the partition being declared recovered.
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_TERMINATIONS_PER_CALL>>,
}

#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, TypeInfo)]
pub struct DeclareFaultsRecoveredParams {
    pub recoveries: BoundedVec<RecoveryDeclaration, ConstU32<DECLARATIONS_MAX>>,
}
