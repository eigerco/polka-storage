use bls12_381::Scalar as Fr;
use num_bigint::BigUint;
use primitives::proofs::RegisteredSealProof;
use sha2::{Digest, Sha256};

use crate::Vec;

/// Identificator of a proof version.
/// There are multiple of those of FileCoin and we need to be compatible with them.
/// The PoRepID used for Proof generation must be the same as the one used for Proof verification (here).
pub type PoRepID = [u8; 32];

/// Configuration used for Proof of Replication.
/// It contains all the necessary data required to construct a PoRep proof scheme that is able to validate the proof.
pub struct Config {
    porep_id: PoRepID,
    nodes: usize,
    challenges: InteractiveChallenges,
}

/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/challenges.rs#L17>
pub struct InteractiveChallenges {
    challenges_per_partition: usize,
}

impl InteractiveChallenges {
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/challenges.rs#L22>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/parameters.rs#L111>
    pub fn new(partitions: usize, minimum_total_challenges: usize) -> Self {
        let challenges_per_partition = usize::div_ceil(minimum_total_challenges, partitions);
        Self {
            challenges_per_partition,
        }
    }

    // Returns the porep challenges for partition `k`.
    // References:
    // * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/challenges.rs#L35>
    pub fn derive(
        &self,
        sector_nodes: usize,
        replica_id: &Fr,
        seed: &[u8; 32],
        k: u8,
    ) -> Vec<usize> {
        (0..self.challenges_per_partition)
            .map(|i| {
                let j: u32 = ((self.challenges_per_partition * k as usize) + i) as u32;

                let hash = Sha256::new()
                    // TODO(@th7nder,23/09/2024): not sure this to_byte, originally it is into_bytes and I don't know where this comes from
                    .chain_update(replica_id.to_bytes())
                    .chain_update(seed)
                    .chain_update(j.to_le_bytes())
                    .finalize();

                let bigint = BigUint::from_bytes_le(hash.as_ref());
                bigint_to_challenge(bigint, sector_nodes)
            })
            .collect()
    }
}

impl Config {
    pub fn new(seal_proof: RegisteredSealProof) -> Self {
        let partitions = partitions(seal_proof);
        Self {
            porep_id: seal_proof.porep_id(),
            // PRE-COND: sector size must be divisible by 32
            nodes: (seal_proof.sector_size().bytes() / 32) as usize,
            challenges: InteractiveChallenges::new(partitions, minimum_challenges(seal_proof)),
        }
    }

    pub fn porep_id(&self) -> PoRepID {
        self.porep_id
    }

    /// Expected number of nodes in the base Depth Robust Graph
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/parameters.rs#L89>
    pub fn nodes(&self) -> usize {
        self.nodes
    }

    pub fn challenges(&self, leaves: usize, replica_id: &Fr, seed: &[u8; 32], k: u8) -> Vec<usize> {
        self.challenges.derive(leaves, replica_id, seed, k)
    }
}

/// Reference:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/constants.rs#L65>
fn partitions(seal_proof: RegisteredSealProof) -> usize {
    match seal_proof {
        RegisteredSealProof::StackedDRG2KiBV1P1 => 1,
        RegisteredSealProof::StackedDRG8MiBV1 => 1,
    }
}

/// Reference:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/constants.rs#L123>
fn minimum_challenges(seal_proof: RegisteredSealProof) -> usize {
    match seal_proof {
        RegisteredSealProof::StackedDRG2KiBV1P1 => 2,
        RegisteredSealProof::StackedDRG8MiBV1 => 2,
    }
}

/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/challenges.rs#L9>
#[inline]
fn bigint_to_challenge(bigint: BigUint, sector_nodes: usize) -> usize {
    debug_assert!(sector_nodes < 1 << 32);
    // Ensure that we don't challenge the first node.
    let non_zero_node = (bigint % (sector_nodes - 1)) + 1usize;
    non_zero_node.to_u32_digits()[0] as usize
}
