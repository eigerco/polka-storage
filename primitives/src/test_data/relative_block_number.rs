use core::ops::{Add, Div, Mul, Sub};

use sp_runtime::traits::{One, Zero};

#[derive(Debug, Copy, Clone)]
pub struct Relative<BlockNumber>(pub BlockNumber);

impl<BlockNumber> Relative<BlockNumber> {
    pub fn zero() -> Self
    where
        BlockNumber: Zero,
    {
        Self(BlockNumber::zero())
    }

    pub fn one() -> Self
    where
        BlockNumber: One,
    {
        Self(BlockNumber::one())
    }
}

impl<BlockNumber> From<BlockNumber> for Relative<BlockNumber> {
    fn from(block_number: BlockNumber) -> Self {
        Self(block_number)
    }
}

impl<BlockNumber> Add<Relative<BlockNumber>> for Relative<BlockNumber>
where
    BlockNumber: Add<BlockNumber, Output = BlockNumber>,
{
    type Output = Self;

    fn add(self, rhs: Relative<BlockNumber>) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl<BlockNumber> Sub<Relative<BlockNumber>> for Relative<BlockNumber>
where
    BlockNumber: Sub<BlockNumber, Output = BlockNumber> + PartialOrd,
{
    type Output = Self;

    fn sub(self, rhs: Relative<BlockNumber>) -> Self::Output {
        assert!(self.0 > rhs.0);
        Self(self.0 - rhs.0)
    }
}

impl<BlockNumber, N> Mul<N> for Relative<BlockNumber>
where
    BlockNumber: Mul<BlockNumber, Output = BlockNumber> + TryFrom<N>,
{
    type Output = Self;

    fn mul(self, rhs: N) -> Self::Output {
        let rhs: BlockNumber = rhs
            .try_into()
            .unwrap_or_else(|_| panic!("failed to convert block number"));
        let result: BlockNumber = self.0 * rhs;
        Self(result)
    }
}

impl<BlockNumber> Div<Relative<BlockNumber>> for Relative<BlockNumber>
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
