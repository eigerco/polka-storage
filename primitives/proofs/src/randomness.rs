extern crate alloc;
use alloc::vec::Vec;

use sp_core::blake2_256;

/// Specifies a domain for randomness generation.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum DomainSeparationTag {
    SealRandomness,
    InteractiveSealChallengeSeed,
    WindowedPoStChallengeSeed,
}

impl DomainSeparationTag {
    /// Returns the domain separation tag as a byte array.
    pub fn as_bytes(&self) -> [u8; 8] {
        let value: i64 = match self {
            DomainSeparationTag::SealRandomness => 1,
            DomainSeparationTag::InteractiveSealChallengeSeed => 2,
            DomainSeparationTag::WindowedPoStChallengeSeed => 3,
        };

        value.to_be_bytes()
    }
}

pub fn draw_randomness(
    rbase: &[u8; 32],
    pers: DomainSeparationTag,
    block_number: u64,
    entropy: &[u8],
) -> [u8; 32] {
    // 8(pers) + 32(rbase) + 8(round) + entropy.len()
    let mut data = Vec::with_capacity(8 + 32 + 8 + entropy.len());

    // Append the personalization value
    let pers = pers.as_bytes();
    data.extend_from_slice(&pers);

    // Append the randomness
    data.extend_from_slice(rbase);

    // Append the round
    let block_number_bytes = block_number.to_be_bytes();
    data.extend_from_slice(&block_number_bytes);

    // Append the entropy
    data.extend_from_slice(entropy);

    // Hash the data
    let mut hashed = blake2_256(&data);
    // Necessary to be valid bls12 381 element.
    hashed[31] &= 0x3f;
    hashed
}

#[cfg(test)]
mod tests {
    use crate::randomness::{draw_randomness, DomainSeparationTag};

    #[test]
    fn draw_randomness_test() {
        let expected_randomness = [
            16, 4, 148, 26, 85, 39, 23, 237, 122, 218, 235, 235, 69, 17, 177, 142, 200, 107, 127,
            84, 189, 40, 145, 187, 205, 159, 58, 161, 209, 57, 226, 4,
        ];

        let digest = [
            24, 234, 27, 198, 74, 225, 75, 88, 98, 20, 13, 68, 97, 66, 153, 51, 124, 108, 201, 87,
            242, 229, 124, 183, 109, 13, 32, 44, 249, 222, 113, 139,
        ];

        let entropy = [68, 0, 153, 203, 52];

        assert_eq!(
            expected_randomness,
            draw_randomness(
                &digest,
                DomainSeparationTag::SealRandomness,
                0_u64,
                entropy.as_slice(),
            )
        );
    }
}
