mod config;

use config::{Config, PoRepID};
use primitives_proofs::{ProverId, RawCommitment, RegisteredSealProof, SectorNumber, Ticket};
use sha2::{Digest, Sha256};

use crate::{
    crypto::groth16::{
        verify_proof, Bls12, Fr, PrimeField, Proof, VerificationError, VerifyingKey,
    },
    fr32,
    graphs::{
        bucket::{BucketGraph, BucketGraphSeed},
        stacked::{StackedBucketGraph, EXP_DEGREE},
    },
    vec, Error, Vec,
};

/// A unique 32-byte ID assigned to each distinct replica.
/// Replication is the entire process by which a sector is uniquely encoded into a replica.
pub type ReplicaId = Fr;

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

pub struct ProofScheme {
    config: Config,
    graph: StackedBucketGraph,
}

#[derive(core::fmt::Debug)]
pub enum ProofError {
    /// Returned when the given proof was invalid in a verification.
    InvalidProof,
    /// Returned when the given verifying key was invalid.
    InvalidVerifyingKey,
    /// Returned in case of failed conversion, i.e. in `bytes_into_fr()`.
    Conversion,
}

impl From<VerificationError> for ProofError {
    fn from(value: VerificationError) -> Self {
        match value {
            VerificationError::InvalidProof => ProofError::InvalidProof,
            VerificationError::InvalidVerifyingKey => ProofError::InvalidVerifyingKey,
        }
    }
}

impl<T> From<ProofError> for Error<T> {
    fn from(value: ProofError) -> Self {
        match value {
            ProofError::InvalidProof => Error::<T>::InvalidPoRepProof,
            ProofError::InvalidVerifyingKey => Error::<T>::InvalidVerifyingKey,
            ProofError::Conversion => Error::<T>::Conversion,
        }
    }
}

pub struct Tau {
    comm_d: Fr,
    comm_r: Fr,
}

pub struct PublicInputs {
    replica_id: ReplicaId,
    tau: Tau,
    seed: Ticket,
}

impl ProofScheme {
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L1020>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/parameters.rs#L69>
    pub fn setup(registered_seal: RegisteredSealProof) -> Self {
        let config = Config::new(registered_seal);
        let drg = BucketGraph::new(config.nodes(), derive_drg_seed(config.porep_id()))
            .expect("properly configured graph");
        let graph = StackedBucketGraph::new(drg, derive_feistel_keys(config.porep_id()));

        Self { config, graph }
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/storage-proofs-core/src/compound_proof.rs#L148>
    pub fn verify(
        &self,
        comm_r: &RawCommitment,
        comm_d: &RawCommitment,
        prover_id: &ProverId,
        sector: SectorNumber,
        ticket: &Ticket,
        seed: &Ticket,
        vk: VerifyingKey<Bls12>,
        proof: &Proof<Bls12>,
    ) -> Result<(), ProofError> {
        let comm_d_fr = fr32::bytes_into_fr(comm_d).map_err(|_| ProofError::Conversion)?;
        let comm_r_fr = fr32::bytes_into_fr(comm_r).map_err(|_| ProofError::Conversion)?;

        let replica_id = self.generate_replica_id(prover_id, sector, ticket, comm_d);
        let public_inputs = PublicInputs {
            replica_id,
            tau: Tau {
                comm_d: comm_d_fr,
                comm_r: comm_r_fr,
            },
            seed: *seed,
        };

        let public_inputs = self.generate_public_inputs(public_inputs, None)?;

        verify_proof(vk, proof, public_inputs.as_slice()).map_err(Into::<ProofError>::into)
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/params.rs#L1266>
    pub fn generate_replica_id(
        &self,
        prover_id: &ProverId,
        sector: SectorNumber,
        ticket: &Ticket,
        comm_d: &RawCommitment,
    ) -> ReplicaId {
        let hash = Sha256::new()
            .chain_update(prover_id)
            .chain_update(u64::from(sector).to_be_bytes())
            .chain_update(ticket)
            .chain_update(comm_d)
            .chain_update(self.config.porep_id())
            .finalize();

        // https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-hashers/src/sha256.rs#L91
        Fr::from_repr_vartime(fr32::bytes_into_fr_repr_safe(hash.as_ref()))
            .expect("bytes_into_fr_repr_safe makes it impossible to fail")
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/circuit/proof.rs#L198>
    pub fn generate_public_inputs(
        &self,
        public_inputs: PublicInputs,
        partition_index: Option<usize>,
    ) -> Result<Vec<Fr>, ProofError> {
        let k = partition_index.unwrap_or(0);
        let PublicInputs {
            replica_id,
            tau: Tau { comm_d, comm_r },
            seed,
        } = public_inputs;
        let leaves = self.graph.size();
        let challenges = self.config.challenges(leaves, &replica_id, &seed, k as u8);

        let mut inputs = Vec::new();
        inputs.push(replica_id);
        inputs.push(comm_d);
        inputs.push(comm_r);

        for challenge in challenges {
            // comm_d inclusion proof for the data leaf
            inputs.push(generate_inclusion_input(challenge));

            // drg parents
            let mut drg_parents = vec![0; self.graph.base_degree()];
            self.graph.base_parents(challenge, &mut drg_parents);

            // Inclusion Proofs: drg parent node in comm_c
            for parent in drg_parents {
                inputs.push(generate_inclusion_input(parent as usize));
            }

            let mut exp_parents = vec![0; EXP_DEGREE];
            self.graph.expanded_parents(challenge, &mut exp_parents);

            // Inclusion Proofs: expander parent node in comm_c
            for parent in exp_parents.into_iter() {
                inputs.push(generate_inclusion_input(parent as usize));
            }

            inputs.push(Fr::from(challenge as u64));

            // Inclusion Proof: encoded node in comm_r_last
            inputs.push(generate_inclusion_input(challenge));

            // Inclusion Proof: column hash of the challenged node in comm_c
            inputs.push(generate_inclusion_input(challenge));
        }

        Ok(inputs)
    }
}

/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/gadgets/por.rs#L310>
fn generate_inclusion_input(challenge: usize) -> Fr {
    // Inputs are (currently, inefficiently) packed with one `Fr` per challenge.
    // Boolean/bit auth paths trivially correspond to the challenged node's index within a sector.
    // Defensively convert the challenge with `try_from` as a reminder that we must not truncate.
    Fr::from(u64::try_from(challenge).expect("challenge type too wide"))
}

#[cfg(test)]
mod tests {
    use primitives_proofs::{RegisteredSealProof, SectorNumber};

    use super::{ProofScheme, PublicInputs, Tau};

    #[test]
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L1012>
    fn generates_public_inputs_the_same_as_reference_impl_2kb_sector() {
        // random numbers, not 0
        let prover_id = [77u8; 32];
        let sector_id = SectorNumber::new(123).unwrap();
        let ticket = [10u8; 32];
        let seed = [10u8; 32];
        let comm_d = [15u8; 32];
        let comm_r = [151u8; 32];

        let inputs =
            ported_generate_public_inputs(&prover_id, sector_id, &ticket, &seed, &comm_d, &comm_r);
        let reference_inputs =
            reference_generate_public_inputs(prover_id, sector_id, ticket, seed, comm_d, comm_r);

        assert_eq!(reference_inputs.len(), inputs.len());
        for index in 0..reference_inputs.len() {
            // blstrs is based on bls12_381 implementation, so we can compare serialized bytes.
            assert_eq!(
                reference_inputs[index].to_bytes_le(),
                inputs[index].to_bytes()
            );
        }
    }

    fn ported_generate_public_inputs(
        prover_id: &[u8; 32],
        sector_id: SectorNumber,
        ticket: &[u8; 32],
        seed: &[u8; 32],
        comm_d: &[u8; 32],
        comm_r: &[u8; 32],
    ) -> Vec<bls12_381::Scalar> {
        let proof_scheme = ProofScheme::setup(RegisteredSealProof::StackedDRG2KiBV1P1);
        let replica_id = proof_scheme.generate_replica_id(prover_id, sector_id, ticket, comm_d);
        // `bytes_into_fr_repr_safe` makes sure random values are convertable into Fr
        let comm_d_fr =
            crate::fr32::bytes_into_fr(&crate::fr32::bytes_into_fr_repr_safe(comm_d)).unwrap();
        let comm_r_fr =
            crate::fr32::bytes_into_fr(&crate::fr32::bytes_into_fr_repr_safe(comm_r)).unwrap();

        let public_inputs = PublicInputs {
            replica_id,
            tau: Tau {
                comm_d: comm_d_fr,
                comm_r: comm_r_fr,
            },
            seed: seed.clone(),
        };

        proof_scheme
            .generate_public_inputs(public_inputs, None)
            .unwrap()
    }

    fn reference_generate_public_inputs(
        prover_id: [u8; 32],
        sector_id: SectorNumber,
        ticket: [u8; 32],
        seed: [u8; 32],
        comm_d: [u8; 32],
        comm_r: [u8; 32],
    ) -> Vec<blstrs::Scalar> {
        use filecoin_hashers::{poseidon::PoseidonHasher, sha256::Sha256Hasher};
        use generic_array::typenum::{U0, U8};
        use storage_proofs_core::{
            api_version::ApiVersion, compound_proof::CompoundProof, drgraph::BASE_DEGREE,
            merkle::LCTree, proof::ProofScheme, util::NODE_SIZE,
        };
        use storage_proofs_porep::stacked::{
            generate_replica_id, Challenges, PublicInputs, SetupParams, StackedCompound,
            StackedDrg, Tau, EXP_DEGREE,
        };

        // https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/constants.rs#L192C28-L192C66
        type SectorShapeBase = LCTree<PoseidonHasher, U8, U0, U0>;
        let setup_params = SetupParams {
            // https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/constants.rs#L18
            nodes: (1 << 11) / NODE_SIZE,
            degree: BASE_DEGREE,
            expansion_degree: EXP_DEGREE,
            // https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L53
            porep_id: [0u8; 32],
            // https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/constants.rs#L123
            challenges: Challenges::new_interactive(2),
            // https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/constants.rs#L84
            num_layers: 2,
            api_version: ApiVersion::V1_2_0,
            api_features: vec![],
        };

        let public_params =
            StackedDrg::<SectorShapeBase, Sha256Hasher>::setup(&setup_params).unwrap();
        let porep_id = [0u8; 32];

        let replica_id = generate_replica_id::<PoseidonHasher, _>(
            &prover_id,
            sector_id.into(),
            &ticket,
            comm_d,
            &porep_id,
        );

        let comm_r_safe = fr32::bytes_into_fr_repr_safe(&comm_r).into();
        let comm_d_safe = fr32::bytes_into_fr_repr_safe(&comm_d).into();

        let public_inputs = PublicInputs::<
            <PoseidonHasher as filecoin_hashers::Hasher>::Domain,
            <Sha256Hasher as filecoin_hashers::Hasher>::Domain,
        > {
            replica_id,
            tau: Some(Tau {
                comm_d: comm_d_safe,
                comm_r: comm_r_safe,
            }),
            seed: Some(seed),
            k: None,
        };

        StackedCompound::<SectorShapeBase, Sha256Hasher>::generate_public_inputs(
            &public_inputs,
            &public_params,
            None,
        )
        .unwrap()
    }
}
