/// Specifies a domain for randomness generation.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
#[repr(i64)]
pub enum DomainSeparationTag {
    SealRandomness = 1,
    InteractiveSealChallengeSeed = 2,
}

pub fn get_randomness(domain: DomainSeparationTag) -> [u8; 32] {
    todo!();
}
