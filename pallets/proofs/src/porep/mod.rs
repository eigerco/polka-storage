mod config;

use config::{Config, PoRepID};
use primitives_proofs::RegisteredSealProof;
use sha2::{Digest, Sha256};

use crate::graphs::bucket::{BucketGraph, BucketGraphSeed, BASE_DEGREE};

/// Serves as a separator for random number generator used for construction of graphs.
/// It makes sure that different seed is used for the same [`PoRepID`], but different Graph construction.
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/crypto/mod.rs#L8>
struct DomainSeparationTag(&'static str);

const DRSAMPLE_DST: DomainSeparationTag = DomainSeparationTag("Filecoin_DRSample");
const FEISTEL_DST: DomainSeparationTag = DomainSeparationTag("Filecoin_Feistel");

/// Creates a seed for RNG, used for [`BucketGraph`] generation.
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L247C1-L252C2>
fn derive_drg_seed(porep_id: PoRepID) -> BucketGraphSeed {
    let mut drg_seed: BucketGraphSeed = [0; 28];
    let raw_seed = derive_porep_domain_seed(DRSAMPLE_DST, porep_id);
    drg_seed.copy_from_slice(&raw_seed[..28]);
    drg_seed
}

/// Creates a seed for RNG, used for [`StackedBucketGraph`] generation.
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/crypto/mod.rs#L13>
fn derive_porep_domain_seed(
    domain_separation_tag: DomainSeparationTag,
    porep_id: PoRepID,
) -> [u8; 32] {
    Sha256::new()
        .chain_update(domain_separation_tag.0)
        .chain_update(porep_id)
        .finalize()
        .into()
}

/// Feistel Cipher is used for [`StackedBucketGraph`] generation.
/// Keys are derived deterministically based on `porep_id`.
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L80C1-L92C1>
/// * <https://en.wikipedia.org/wiki/Feistel_cipher#Theoretical_work>
pub fn derive_feistel_keys(porep_id: PoRepID) -> [u64; 4] {
    let mut feistel_keys = [0u64; 4];
    let raw_seed = derive_porep_domain_seed(FEISTEL_DST, porep_id);
    feistel_keys[0] = u64::from_le_bytes(raw_seed[0..8].try_into().expect("seed to have 32 bytes"));
    feistel_keys[1] =
        u64::from_le_bytes(raw_seed[8..16].try_into().expect("seed to have 32 bytes"));
    feistel_keys[2] =
        u64::from_le_bytes(raw_seed[16..24].try_into().expect("seed to have 32 bytes"));
    feistel_keys[3] =
        u64::from_le_bytes(raw_seed[24..32].try_into().expect("seed to have 32 bytes"));
    feistel_keys
}

pub struct ProofScheme;

impl ProofScheme {
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L1020>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/parameters.rs#L69>
    pub fn setup(registered_seal: RegisteredSealProof) -> Self {
        let config = Config::new(registered_seal);

        let drg = BucketGraph::new(config.nodes(), derive_drg_seed(config.porep_id()))
            .expect("properly configured graph");
        // Just as showcase to ignore unused warnings for now.
        let mut parents = [0; BASE_DEGREE];
        drg.parents(0, &mut parents);

        let _feistel_keys = derive_feistel_keys(config.porep_id());

        Self
    }

    pub fn verify(&self) {
        todo!();
    }
}
