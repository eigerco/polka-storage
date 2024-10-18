extern crate alloc;
use alloc::vec::Vec;

use sp_core::blake2_256;

/// Specifies a domain for randomness generation.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
#[repr(i64)]
pub enum DomainSeparationTag {
    SealRandomness = 1,
    InteractiveSealChallengeSeed = 2,
}

pub fn draw_randomness<BlocKNumber>(
    rbase: &[u8; 32],
    pers: DomainSeparationTag,
    round: BlocKNumber,
    entropy: &[u8],
) -> [u8; 32]
where
    BlocKNumber: sp_runtime::traits::BlockNumber,
{
    // 8(pers) + 32(rbase) + 8(round) + entropy.len()
    let mut data = Vec::with_capacity(8 + 32 + 8 + entropy.len());

    // Append the personalization value
    let pers = (pers as i64).to_be_bytes();
    data.extend_from_slice(&pers);

    // Append the randomness
    data.extend_from_slice(rbase);

    // Append the round
    round.encode_to(&mut data);

    // Append the entropy
    data.extend_from_slice(entropy);

    // Hash the data
    blake2_256(&data)
}

#[cfg(test)]
mod tests {
    use crate::randomness::{draw_randomness, DomainSeparationTag};

    #[test]
    fn draw_randomness_test() {
        let expected_randomness = [
            140, 200, 74, 146, 51, 100, 10, 170, 108, 137, 128, 227, 6, 228, 100, 1, 137, 133, 222,
            5, 250, 41, 152, 9, 229, 132, 167, 239, 215, 24, 223, 165,
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
                2797727_u64,
                entropy.as_slice(),
            )
        );
    }
}
