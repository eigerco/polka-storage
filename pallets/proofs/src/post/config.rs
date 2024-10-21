use primitives_proofs::RegisteredPoStProof;

pub struct Config {
    /// Size of the sector in bytes.
    pub sector_size: u64,
    /// Number of challenges per sector (challenge_count).
    pub challenges_per_sector: usize,
    /// Number of challenged sectors (sector_count).
    pub challenged_sectors_per_partition: usize,
}

impl Config {
    pub fn new(post_type: RegisteredPoStProof) -> Self {
        Self {
            sector_size: post_type.sector_size().bytes(),
            challenges_per_sector: WINDOW_POST_CHALLENGE_COUNT,
            challenged_sectors_per_partition: sector_count(post_type),
        }
    }
}

/// The number of challenges generated for a single sector.
///
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L32>
const WINDOW_POST_CHALLENGE_COUNT: usize = 10;


/// Number of sectors challenged in a replica.
///
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L102>
fn sector_count(post_type: RegisteredPoStProof) -> usize {
    match post_type {
        RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => 2,
    }
}
