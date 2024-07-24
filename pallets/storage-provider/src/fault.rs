use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeSet};
use primitives_proofs::SectorNumber;
use scale_info::prelude::vec::Vec;

use crate::sector::MAX_SECTORS;

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct FaultDeclaration {
    /// The deadline to which the faulty sectors are assigned, in range [0..WPoStPeriodDeadlines)
    pub deadline: u64,
    /// Partition index within the deadline containing the faulty sectors.
    pub partition: u64,
    /// Sectors in the partition being declared faulty.
    pub sectors: BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct DeclareFaultsParams {
    pub faults: Vec<FaultDeclaration>,
}
