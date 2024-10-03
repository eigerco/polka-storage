use core::ops::{Add, AddAssign, Deref};

use crate::NODE_SIZE;

/// Size of a piece in bytes. Unpadded piece size should be power of two
/// multiple of 127.
#[derive(PartialEq, Debug, Eq, Clone, Copy)]
pub struct UnpaddedPieceSize(u64);

impl UnpaddedPieceSize {
    /// Initialize new unpadded piece size. Error is returned if the size is
    /// invalid.
    pub fn new(size: u64) -> Result<Self, &'static str> {
        if size < 127 {
            return Err("minimum piece size is 127 bytes");
        }

        // is 127 * 2^n
        if size >> size.trailing_zeros() != 127 {
            return Err("unpadded piece size must be a power of 2 multiple of 127");
        }

        Ok(Self(size))
    }

    /// Converts unpadded piece size into padded piece size.
    pub fn padded(self) -> PaddedPieceSize {
        PaddedPieceSize(self.0 + (self.0 / 127))
    }
}

impl Deref for UnpaddedPieceSize {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Add for UnpaddedPieceSize {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        UnpaddedPieceSize(self.0 + other.0)
    }
}

/// Size of a piece in bytes with padding. The size should be power of two.
#[derive(PartialEq, Debug, Eq, Clone, Copy)]
pub struct PaddedPieceSize(u64);

impl PaddedPieceSize {
    /// Initialize new padded piece size. Error is returned if the size is
    /// invalid.
    pub fn new(size: u64) -> Result<Self, &'static str> {
        if size < 128 {
            return Err("minimum piece size is 128 bytes");
        }

        if size.count_ones() != 1 {
            return Err("padded piece size must be a power of 2");
        }

        if size % NODE_SIZE != 0 {
            return Err("padded_piece_size is not multiple of NODE_SIZE");
        }

        Ok(Self(size))
    }

    /// Converts padded piece size into an unpadded piece size.
    pub fn unpadded(self) -> UnpaddedPieceSize {
        UnpaddedPieceSize(self.0 - (self.0 / 128))
    }
}

impl Deref for PaddedPieceSize {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Add for PaddedPieceSize {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        PaddedPieceSize(self.0 + other.0)
    }
}

impl AddAssign for PaddedPieceSize {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl core::iter::Sum for PaddedPieceSize {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(PaddedPieceSize(0), |acc, x| acc + x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_piece_size() {
        let p_piece = PaddedPieceSize::new(0b10000000).unwrap();
        let up_piece = p_piece.unpadded();
        assert_eq!(&up_piece, &UnpaddedPieceSize(127));
        assert_eq!(&p_piece, &up_piece.padded());
    }
    #[test]
    fn invalid_piece_checks() {
        assert_eq!(
            PaddedPieceSize::new(127),
            Err("minimum piece size is 128 bytes")
        );
        assert_eq!(
            UnpaddedPieceSize::new(126),
            Err("minimum piece size is 127 bytes")
        );
        assert_eq!(
            PaddedPieceSize::new(0b10000001),
            Err("padded piece size must be a power of 2")
        );
        assert_eq!(
            UnpaddedPieceSize::new(0b1110111000),
            Err("unpadded piece size must be a power of 2 multiple of 127")
        );
        assert!(UnpaddedPieceSize::new(0b1111111000).is_ok());
    }
}
