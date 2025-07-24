use core::ops::{Add, Div, Rem, Sub};

use sp_runtime::traits::Zero;

use crate::relative_block_number::Relative;

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct Absolute<BlockNumber>(pub BlockNumber);

impl<BlockNumber> Absolute<BlockNumber> {
    pub fn zero() -> Self
    where
        BlockNumber: Zero,
    {
        Self(BlockNumber::zero())
    }
}

impl<BlockNumber> From<BlockNumber> for Absolute<BlockNumber> {
    fn from(block_number: BlockNumber) -> Self {
        Self(block_number)
    }
}

impl<BlockNumber> Add<Relative<BlockNumber>> for Absolute<BlockNumber>
where
    BlockNumber: Add<BlockNumber, Output = BlockNumber>,
{
    type Output = Self;

    fn add(self, rhs: Relative<BlockNumber>) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl<BlockNumber> Sub<Relative<BlockNumber>> for Absolute<BlockNumber>
where
    BlockNumber: Sub<BlockNumber, Output = BlockNumber> + PartialOrd,
{
    type Output = Self;

    fn sub(self, rhs: Relative<BlockNumber>) -> Self::Output {
        assert!(self.0 > rhs.0);
        Self(self.0 - rhs.0)
    }
}

impl<BlockNumber> Sub<Absolute<BlockNumber>> for Absolute<BlockNumber>
where
    BlockNumber: Sub<BlockNumber, Output = BlockNumber> + PartialOrd,
{
    type Output = Relative<BlockNumber>;

    fn sub(self, rhs: Absolute<BlockNumber>) -> Self::Output {
        assert!(self.0 > rhs.0);
        Relative(self.0 - rhs.0)
    }
}

impl<BlockNumber> Rem<Relative<BlockNumber>> for Absolute<BlockNumber>
where
    BlockNumber: Rem<BlockNumber, Output = BlockNumber>,
{
    type Output = Relative<BlockNumber>;

    fn rem(self, rhs: Relative<BlockNumber>) -> Self::Output {
        Relative(self.0 % rhs.0)
    }
}

impl<BlockNumber> Div<Relative<BlockNumber>> for Absolute<BlockNumber>
where
    BlockNumber: Div<BlockNumber, Output = BlockNumber> + TryInto<u64> + Zero,
{
    type Output = u64;

    fn div(self, rhs: Relative<BlockNumber>) -> Self::Output {
        assert!(!rhs.0.is_zero());
        (self.0 / rhs.0)
            .try_into()
            .unwrap_or_else(|_| panic!("block number to fit in a u64"))
    }
}
