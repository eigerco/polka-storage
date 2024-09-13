//! Functionality related to Filecoin's PoRep.

use std::path::PathBuf;

use bellperson::groth16::{Proof, VerifyingKey};
use blstrs::{Bls12, Scalar as Fr};
use filecoin_hashers::Hasher;
use fr32::u64_into_fr;
use merkletree::store::StoreConfig;
use rand::Rng;
use rand_xorshift::XorShiftRng;
use storage_proofs_core::{
    compound_proof::{CompoundProof, PublicParams},
    data::Data,
    drgraph::Graph,
    gadgets::por::PoRCompound,
    merkle::{BinaryMerkleTree, MerkleTreeTrait},
    multi_proof::MultiProof,
    por::{self, PoR},
    proof::ProofScheme,
};
use storage_proofs_porep::stacked::{
    self, ChallengeRequirements, PersistentAux, PrivateInputs, PublicInputs, StackedCompound,
    StackedDrg, Tau, TemporaryAux, TemporaryAuxCache,
};

// Helpers.
pub type TreeHasher<T> = <T as MerkleTreeTrait>::Hasher;
pub type PubInputs<T, H> = PublicInputs<<TreeHasher<T> as Hasher>::Domain, <H as Hasher>::Domain>;

/// TODO
pub fn generate_porep<'r, TTree, THasher>(
    rng: &mut XorShiftRng,
    pub_params: &'r PublicParams<'r, StackedDrg<'r, TTree, THasher>>,
    store_cfg: &StoreConfig,
    replica_id: <TreeHasher<TTree> as Hasher>::Domain,
    mmapped_data: &mut [u8],
) -> anyhow::Result<(Proof<Bls12>, VerifyingKey<Bls12>, Vec<Fr>)>
where
    TTree: 'static + MerkleTreeTrait, // + ProofScheme<'r, PublicParams=stacked::PublicParams<TTree>>,
    THasher: 'static + Hasher,
{
    let replica_path = crate::utils::replica_path(store_cfg);

    // IMPORTANT: This part here is the local PoRep proof and its verification. We are using a
    //            ZK-SNARK afterwards to proove that we verified it locally and correctly.
    // 1. Generate public parameters.
    // let pub_params = StackedCompound::setup(setup_params).unwrap();
    // 2. Generate public and private inputs.
    let (tau, (p_aux, t_aux)) = transform_and_replicate_layers::<TTree, THasher>(
        &pub_params.vanilla_params,
        &replica_id,
        (mmapped_data.as_mut()).into(),
        store_cfg.path.clone(),
        replica_path.clone(),
    );
    let t_aux = TemporaryAuxCache::<TTree, THasher>::new(&t_aux, replica_path, false).unwrap();
    let pub_inputs = PubInputs::<TTree, THasher> {
        replica_id,
        seed: Some(rng.gen()),
        tau: Some(tau),
        k: Some(0),
    };
    let priv_inputs = PrivateInputs { p_aux, t_aux };
    // 3. Generate the proof and verifying-key (TODO).
    let partitions = 1;
    let proofs = StackedDrg::prove_all_partitions(
        &pub_params.vanilla_params,
        &pub_inputs,
        &priv_inputs,
        partitions,
    )
    .unwrap();
    // 4. Verify locally those proofs.
    assert!(
        StackedDrg::verify_all_partitions(&pub_params.vanilla_params, &pub_inputs, &proofs)
            .unwrap()
    );

    // SECOND STAGE: Build a ZK-SNARK on top of it and verify this on-chain.
    let blank_groth_params = <StackedCompound<TTree, THasher> as CompoundProof<
        StackedDrg<TTree, THasher>,
        _,
    >>::groth_params(Some(rng), &pub_params.vanilla_params)
    .unwrap();
    let proofs =
        StackedCompound::prove(pub_params, &pub_inputs, &priv_inputs, &blank_groth_params).unwrap();

    let verifying_key = blank_groth_params.vk;
    let prepared_verifying_key = blank_groth_params.pvk;
    let multi_proof = MultiProof::new(proofs.clone(), &prepared_verifying_key);

    // This needs to be done on-chain. Optional: Prepare the key here as well.
    assert!(StackedCompound::verify(
        &pub_params,
        &pub_inputs,
        &multi_proof,
        &ChallengeRequirements {
            minimum_challenges: 1,
        },
    )
    .unwrap());
    println!("Generated and verified proofs locally and successfully ✅");

    let real_pub_inputs =
        generate_public_inputs::<TTree, THasher>(&pub_inputs, &pub_params.vanilla_params, Some(0))
            .unwrap();

    Ok((
        multi_proof.circuit_proofs[0].clone(),
        verifying_key,
        real_pub_inputs,
    ))
}

#[allow(clippy::type_complexity)]
fn transform_and_replicate_layers<Tr, G>(
    pp: &stacked::PublicParams<Tr>,
    replica_id: &<Tr::Hasher as Hasher>::Domain,
    data: Data<'_>,
    cache_dir: PathBuf,
    replica_path: PathBuf,
) -> (
    Tau<<Tr::Hasher as Hasher>::Domain, <G as Hasher>::Domain>,
    (
        PersistentAux<<Tr::Hasher as Hasher>::Domain>,
        TemporaryAux<Tr, G>,
    ),
)
where
    Tr: 'static + MerkleTreeTrait,
    G: 'static + Hasher,
{
    let (labels, _) = stacked::StackedDrg::<Tr, G>::replicate_phase1(pp, replica_id, &cache_dir)
        .expect("failed to generate labels");
    stacked::StackedDrg::replicate_phase2(pp, labels, data, None, cache_dir, replica_path)
        .expect("failed to transform")
}

fn generate_public_inputs<TTree, THasher>(
    pub_inputs: &<stacked::StackedDrg<'_, TTree, THasher> as ProofScheme>::PublicInputs,
    pub_params: &<stacked::StackedDrg<'_, TTree, THasher> as ProofScheme>::PublicParams,
    k: Option<usize>,
) -> anyhow::Result<Vec<Fr>>
where
    TTree: MerkleTreeTrait + 'static,
    THasher: Hasher + 'static,
{
    assert!(pub_inputs.seed.is_some());

    let graph = &pub_params.graph;
    let mut inputs = Vec::<Fr>::new();
    let replica_id = pub_inputs.replica_id;
    inputs.push(replica_id.into());
    let comm_d = pub_inputs.tau.as_ref().expect("missing tau").comm_d;
    inputs.push(comm_d.into());
    let comm_r = pub_inputs.tau.as_ref().expect("missing tau").comm_r;
    inputs.push(comm_r.into());

    let por_setup_params = por::SetupParams {
        leaves: graph.size(),
        private: true,
    };
    let por_params = PoR::<TTree>::setup(&por_setup_params)?;
    let por_params_d = PoR::<BinaryMerkleTree<THasher>>::setup(&por_setup_params)?;

    let all_challenges = pub_inputs.challenges(&pub_params.challenges, graph.size(), k);

    for challenge in all_challenges.into_iter() {
        inputs.extend(generate_inclusion_inputs::<BinaryMerkleTree<THasher>>(
            &por_params_d,
            challenge,
            k,
        )?);

        let mut drg_parents = vec![0; graph.base_graph().degree()];
        graph.base_graph().parents(challenge, &mut drg_parents)?;

        for parent in drg_parents.into_iter() {
            inputs.extend(generate_inclusion_inputs::<TTree>(
                &por_params,
                parent as usize,
                k,
            )?);
        }

        let mut exp_parents = vec![0; graph.expansion_degree()];
        graph.expanded_parents(challenge, &mut exp_parents)?;

        for parent in exp_parents.into_iter() {
            inputs.extend(generate_inclusion_inputs::<TTree>(
                &por_params,
                parent as usize,
                k,
            )?);
        }

        inputs.push(u64_into_fr(challenge as u64));

        inputs.extend(generate_inclusion_inputs::<TTree>(
            &por_params,
            challenge,
            k,
        )?);

        inputs.extend(generate_inclusion_inputs::<TTree>(
            &por_params,
            challenge,
            k,
        )?);
    }

    Ok(inputs)
}

fn generate_inclusion_inputs<Tree: 'static + MerkleTreeTrait>(
    por_params: &por::PublicParams,
    challenge: usize,
    k: Option<usize>,
) -> anyhow::Result<Vec<Fr>> {
    let pub_inputs = por::PublicInputs::<<Tree::Hasher as Hasher>::Domain> {
        challenge,
        commitment: None,
    };
    PoRCompound::<Tree>::generate_public_inputs(&pub_inputs, por_params, k)
}
