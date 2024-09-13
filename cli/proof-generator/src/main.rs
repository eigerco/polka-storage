use std::{fs::File, io::prelude::*, path::PathBuf};

use bellperson::groth16;
use bls12_381::Bls12;
use blstrs::Scalar as Fr;
use ff::Field;
use filecoin_hashers::{Domain, Hasher};
use filecoin_proofs::{DefaultPieceHasher, SectorShape2KiB};
use fr32::{fr_into_bytes, u64_into_fr};
use merkletree::store::StoreConfig;
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;
use storage_proofs_core::{
    api_version::ApiVersion,
    cache_key::CacheKey,
    compound_proof::{self, CompoundProof},
    data::Data,
    drgraph::{Graph, BASE_DEGREE},
    gadgets::por::PoRCompound,
    merkle::{get_base_tree_count, BinaryMerkleTree, MerkleTreeTrait},
    multi_proof::MultiProof,
    por::{self, PoR},
    proof::ProofScheme,
    test_helper::setup_replica,
};
use storage_proofs_porep::stacked::{
    self, ChallengeRequirements, Challenges, PersistentAux, PrivateInputs, PublicInputs,
    PublicParams, SetupParams, StackedCompound, Tau, TemporaryAux, TemporaryAuxCache,
};
use tempfile::tempdir;

mod pallet;

use pallet::{PtPreparedVerifyingKey, PtProof, PtPublicInputs, PtVerifyingKey};

// const SECTOR_SIZE_2_KIB: u64 = 1 << 11;
const TEST_SEED: [u8; 16] = [
    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
];
const EXP_DEGREE: usize = 8;

// Selectable types.
type TTree = SectorShape2KiB;
// Fixed types to simplify things.
type THasher = DefaultPieceHasher;
type TreeHasher = <TTree as MerkleTreeTrait>::Hasher;
type StackedDrg = stacked::StackedDrg<'static, TTree, THasher>;
type PubInputs = PublicInputs<<TreeHasher as Hasher>::Domain, <THasher as Hasher>::Domain>;

fn main() {
    println!("Trying to generate proofs, verifying-key and public inputs... 💻");

    // Parameterisation.
    let challenges = Challenges::new_interactive(1);
    let num_layers = 2;
    let partition_count = 1;

    // Preparation for SetupParameters.
    let cache_dir = tempdir().unwrap();
    let store_cfg = StoreConfig::new(cache_dir.path(), CacheKey::CommDTree.to_string(), 0);
    let replica_path = cache_dir.path().join("replicate-path");

    let mut rng = XorShiftRng::from_seed(TEST_SEED);
    let nodes = 8 * get_base_tree_count::<TTree>();
    let setup_params = compound_proof::SetupParams::<StackedDrg> {
        vanilla_params: SetupParams {
            nodes,
            degree: BASE_DEGREE,
            expansion_degree: EXP_DEGREE,
            porep_id: generate_random_id(&mut rng),
            challenges,
            num_layers,
            api_version: ApiVersion::V1_2_0,
            api_features: vec![],
        },
        partitions: Some(partition_count),
        priority: false,
    };
    let replica_id = <TreeHasher as Hasher>::Domain::random(&mut rng);
    let data: Vec<u8> = (0..nodes)
        .flat_map(|_| fr_into_bytes(&Fr::random(&mut rng)))
        .collect();
    let mut mmapped_data = setup_replica(&data, &replica_path);

    // IMPORTANT: This part here is the local PoRep proof and its verification. We are using a
    //            ZK-SNARK afterwards to proove that we verified it locally and correctly.
    // 1. Generate public parameters.
    let pub_params = StackedCompound::setup(&setup_params).unwrap();
    // 2. Generate public and private inputs.
    let (tau, (p_aux, t_aux)) = transform_and_replicate_layers::<TTree, THasher>(
        &pub_params.vanilla_params,
        &replica_id,
        (mmapped_data.as_mut()).into(),
        store_cfg.path,
        replica_path.clone(),
    );
    let t_aux = TemporaryAuxCache::<TTree, THasher>::new(&t_aux, replica_path, false).unwrap();
    stacked::clear_cache_dir(cache_dir.path()).expect("expect cache to be cleared");
    cache_dir.close().unwrap();
    let pub_inputs = PubInputs {
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
    let blank_groth_params =
        <StackedCompound<TTree, THasher> as CompoundProof<StackedDrg, _>>::groth_params(
            Some(&mut rng),
            &pub_params.vanilla_params,
        )
        .unwrap();
    let proofs =
        StackedCompound::prove(&pub_params, &pub_inputs, &priv_inputs, &blank_groth_params)
            .unwrap();

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

    // The real publix input parameters look a bit different.
    println!("Converting outputs to pallet-compatible format... 💻");
    let mut real_pub_inputs =
        generate_public_inputs(&pub_inputs, &pub_params.vanilla_params, Some(0)).unwrap();

    // Serialise and send to pallet storage-provider.
    let pt_vkey = pallet::into_pallet_verifying_key(&verifying_key).unwrap();
    let pt_proof = pallet::into_pallet_proof(&multi_proof.circuit_proofs).unwrap();
    let pt_proof = pt_proof[0].clone();
    let pt_pub_inputs = pallet::into_pallet_public_inputs(&real_pub_inputs).unwrap();

    println!("real_pub_inputs: {}, verifying_key: {}", real_pub_inputs.len(), verifying_key.ic.len());
    println!("pt_pub_inputs: {}, pt_vkey: {}", pt_pub_inputs.0.len() +  1, pt_vkey.ic.len());
    assert!(
        pt_pub_inputs.0.len() + 1 == pt_vkey.ic.len(),
        "Invalid VerifyingKey, num(pub-inputs) + 1 != num(vkey.ic) ❌"
    );

    let pt_vkey_bytes = pt_vkey.into_bytes();
    let pt_proof_bytes = pt_proof.into_bytes();
    let pt_inputs_bytes = pt_pub_inputs.into_bytes();

    println!("Verifying conversion to pallet-compatible formats... 💻");
    let pt_vkey = PtVerifyingKey::<Bls12>::from_bytes(&pt_vkey_bytes).unwrap();
    let pt_pvk = PtPreparedVerifyingKey::<Bls12>::from(pt_vkey);
    let pt_proof = PtProof::<Bls12>::from_bytes(&pt_proof_bytes).unwrap();
    let pt_inputs = PtPublicInputs::<Bls12>::from_bytes(&pt_inputs_bytes).unwrap();
    assert!(
        pallet_storage_provider::proofs::verify_proof(&pt_pvk, &pt_proof, &pt_inputs).is_ok(),
        "Could not verify proof via pallet implementation ❌"
    );
    println!("VerifyingKey, Proof and PublicInputs are pallet-convertible ✅");

    // println!("Proof:");
    // println!("{}", hex::encode(&pt_proof_bytes));
    // pt_proof_bytes.iter().for_each(|b| print!("{b:02x} "));
    // pt_proof_bytes.iter().for_each(|b| print!("{b} "));
    let mut file = File::create("proof").unwrap();
    file.write_all(&pt_proof_bytes).unwrap();

    // println!("\nVerifying-Key:");
    // println!("{}", hex::encode(&pt_vkey_bytes));
    // pt_vkey_bytes.iter().for_each(|b| print!("{b:02x} "));
    // pt_vkey_bytes.iter().for_each(|b| print!("{b} "));
    let mut file = File::create("vkey").unwrap();
    file.write_all(&pt_vkey_bytes).unwrap();

    // println!("\nPublic Inputs:");
    // println!("{}", hex::encode(&pt_inputs_bytes));
    // pt_inputs_bytes.iter().for_each(|b| print!("{b:02x} "));
    // pt_inputs_bytes.iter().for_each(|b| print!("{b} "));
    let mut file = File::create("inputs").unwrap();
    file.write_all(&pt_inputs_bytes).unwrap();

    println!("Serialised items written to files: 'proof', 'vkey' and 'input' ✅");
}

fn generate_public_inputs(
    pub_inputs: &<stacked::StackedDrg<'_, TTree, THasher> as ProofScheme>::PublicInputs,
    pub_params: &<stacked::StackedDrg<'_, TTree, THasher> as ProofScheme>::PublicParams,
    k: Option<usize>,
) -> anyhow::Result<Vec<Fr>> {
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

fn generate_random_id(rng: &mut XorShiftRng) -> [u8; 32] {
    let mut id = [0u8; 32];
    let fr: <<SectorShape2KiB as MerkleTreeTrait>::Hasher as Hasher>::Domain =
        Fr::random(rng).into();
    id.copy_from_slice(AsRef::<[u8]>::as_ref(&fr));
    id
}

#[allow(clippy::type_complexity)]
fn transform_and_replicate_layers<Tr: 'static + MerkleTreeTrait, G: 'static + Hasher>(
    pp: &PublicParams<Tr>,
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
) {
    let (labels, _) = stacked::StackedDrg::<Tr, G>::replicate_phase1(pp, replica_id, &cache_dir)
        .expect("failed to generate labels");
    stacked::StackedDrg::replicate_phase2(pp, labels, data, None, cache_dir, replica_path)
        .expect("failed to transform")
}
