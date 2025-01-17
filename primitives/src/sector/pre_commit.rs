//! Pre-commit primitive.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{BoundedVec, RuntimeDebug};

use crate::{
    proofs::RegisteredSealProof, sector::SectorNumber, DealId, CID_SIZE_IN_BYTES,
    MAX_DEALS_PER_SECTOR,
};

/// This type is passed into the pre commit function on the storage provider pallet
#[derive(Clone, RuntimeDebug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub struct SectorPreCommitInfo<BlockNumber> {
    pub seal_proof: RegisteredSealProof,
    /// Which sector number this SP is pre-committing.
    pub sector_number: SectorNumber,
    /// This value is also known as `commR` or "commitment of replication". The terms `commR` and `sealed_cid` are interchangeable.
    /// Using sealed_cid as I think that is more descriptive.
    /// Some docs on commR here: <https://proto.school/verifying-storage-on-filecoin/03>
    pub sealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
    /// The block number at which we requested the randomness when sealing the sector.
    pub seal_randomness_height: BlockNumber,
    /// Deals Ids that are supposed to be activated.
    /// If any of those is invalid, whole activation is rejected.
    pub deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
    /// Expiration of the pre-committed sector.
    pub expiration: BlockNumber,
    /// This value is also known as `commD` or "commitment of data".
    /// Once a sector is full `commD` is produced representing the root node of all of the piece CIDs contained in the sector.
    pub unsealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
}

#[cfg(any(test, feature = "builder"))]
pub mod builder {

    use core::str::FromStr;

    use cid::Cid;
    use sp_core::ConstU32;
    use sp_runtime::BoundedVec;

    use super::SectorPreCommitInfo;
    use crate::{
        proofs::RegisteredSealProof, sector::SectorNumber, DealId, CID_SIZE_IN_BYTES,
        MAX_DEALS_PER_SECTOR,
    };

    /// [`SectorPreCommitInfo`] builder.
    ///
    /// Usage:
    /// ```no_run
    /// // Instances are created with defaults
    /// SectorPreCommitInfoBuilder::default()
    ///     .sector_number(10.into())
    ///     .deals(vec![1000]);
    /// ```
    pub struct SectorPreCommitInfoBuilder<BlockNumber> {
        seal_proof: RegisteredSealProof,
        sector_number: SectorNumber,
        sealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
        deal_ids: BoundedVec<DealId, ConstU32<MAX_DEALS_PER_SECTOR>>,
        expiration: BlockNumber,
        unsealed_cid: BoundedVec<u8, ConstU32<CID_SIZE_IN_BYTES>>,
        seal_randomness_height: BlockNumber,
    }

    impl<BlockNumber> Default for SectorPreCommitInfoBuilder<BlockNumber>
    where
        BlockNumber: sp_runtime::traits::BlockNumber,
    {
        fn default() -> Self {
            let unsealed_cid =
                Cid::from_str("baga6ea4seaqmruupwrxaeck7m3f5jtswpr7jv6bvwqeu5jinzjlcybh6er3ficq")
                    .unwrap()
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes");

            let sealed_cid =
                Cid::from_str("bagboea4b5abcamxmh7exq7vrvacvajooeapagr3a4g3tpjhw73iny47hvafw76gr")
                    .unwrap()
                    .to_bytes()
                    .try_into()
                    .expect("hash is always 32 bytes");

            Self {
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                sector_number: SectorNumber::new(1).unwrap(),
                sealed_cid,
                deal_ids: BoundedVec::try_from(vec![0, 1])
                    .expect("default valid should always be within bounds"),
                expiration: 120u32.into(),
                unsealed_cid,
                seal_randomness_height: BlockNumber::one(),
            }
        }
    }

    impl<BlockNumber> SectorPreCommitInfoBuilder<BlockNumber>
    where
        BlockNumber: sp_runtime::traits::BlockNumber,
    {
        pub fn sector_number(mut self, sector_number: SectorNumber) -> Self {
            self.sector_number = sector_number;
            self
        }

        /// Panics if the length of `deal_ids` is larger than [`MAX_DEALS_PER_SECTOR`].
        pub fn deals(mut self, deal_ids: Vec<u64>) -> Self {
            self.deal_ids = BoundedVec::try_from(deal_ids).unwrap();
            self
        }

        pub fn expiration(mut self, expiration: BlockNumber) -> Self {
            self.expiration = expiration;
            self
        }

        pub fn unsealed_cid(mut self, unsealed_cid: &str) -> Self {
            let cid = Cid::from_str(unsealed_cid).expect("valid unsealed_cid");
            self.unsealed_cid = BoundedVec::try_from(cid.to_bytes()).unwrap();
            self
        }

        pub fn build(self) -> SectorPreCommitInfo<BlockNumber> {
            SectorPreCommitInfo {
                seal_proof: self.seal_proof,
                sector_number: self.sector_number,
                sealed_cid: self.sealed_cid,
                deal_ids: self.deal_ids,
                expiration: self.expiration,
                unsealed_cid: self.unsealed_cid,
                seal_randomness_height: self.seal_randomness_height,
            }
        }
    }
}
