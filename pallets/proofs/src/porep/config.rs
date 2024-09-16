use primitives_proofs::RegisteredSealProof;

/// Identificator of a proof version.
/// There are multiple of those of FileCoin and we need to be compatible with them.
/// The PoRepID used for Proof generation must be the same as the one used for Proof verification (here).
pub type PoRepID = [u8; 32];

/// Configuration used for Proof of Replication.
/// It contains all the necessary data required to construct a PoRep proof scheme that is able to validate the proof.
pub struct Config {
    porep_id: PoRepID,
    nodes: usize,
}

impl Config {
    pub fn new(seal_proof: RegisteredSealProof) -> Self {
        Self {
            porep_id: porep_id(seal_proof),
            // PRE-COND: sector size must be divisible by 32
            nodes: (seal_proof.sector_size().bytes() / 32) as usize,
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
}

/// References:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L283C1-L302C6>
fn nonce(seal_proof: RegisteredSealProof) -> u64 {
    #[allow(clippy::match_single_binding)]
    match seal_proof {
        // If we ever need to change the nonce for any given RegisteredSealProof, match it here.
        _ => 0,
    }
}

/// References:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L292>
pub fn porep_id(seal_proof: RegisteredSealProof) -> PoRepID {
    let mut porep_id = [0; 32];
    let registered_proof_id = proof_id(seal_proof);
    let n = nonce(seal_proof);

    porep_id[0..8].copy_from_slice(&registered_proof_id.to_le_bytes());
    porep_id[8..16].copy_from_slice(&n.to_le_bytes());
    porep_id
}

/// Reference:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L52>
fn proof_id(seal_proof: RegisteredSealProof) -> u64 {
    match seal_proof {
        RegisteredSealProof::StackedDRG2KiBV1P1 => 0,
    }
}
