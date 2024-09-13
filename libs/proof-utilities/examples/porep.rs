use std::{fs::File, io::prelude::*};

// use bls12_381::Bls12;
use blstrs::Scalar as Fr;
use ff::Field;
use filecoin_hashers::{Domain, Hasher};
use filecoin_proofs::{DefaultPieceHasher, SectorShape2KiB};
use fr32::fr_into_bytes;
// #[cfg(test)]
// use pallet_storage_provider::proofs::verify_proof;
use proof_utilities::{pallet, porep, utils};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;
use storage_proofs_core::{
    api_version::ApiVersion,
    compound_proof::{CompoundProof, SetupParams},
    drgraph::BASE_DEGREE,
    merkle::get_base_tree_count,
    test_helper::setup_replica,
};
use storage_proofs_porep::stacked::{self, Challenges, StackedCompound, StackedDrg};

// use pallet::{PtPreparedVerifyingKey, PtProof, PtPublicInputs, PtVerifyingKey};

// const SECTOR_SIZE_2_KIB: u64 = 1 << 11;
const TEST_SEED: [u8; 16] = [
    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
];
const EXP_DEGREE: usize = 8;

// Selectable types.
type TTree = SectorShape2KiB;
// Fixed types to simplify things.
type THasher = DefaultPieceHasher;

fn main() {
    println!("Trying to generate proofs, verifying-key and public inputs... 💻");

    // More or less fixed things.
    let mut rng = XorShiftRng::from_seed(TEST_SEED);
    let store_cfg = utils::store_config();
    let replica_path = utils::replica_path(&store_cfg);
    let nodes = 8 * get_base_tree_count::<TTree>();

    // Remaining parameterisation.
    let challenges = 1;
    let num_layers = 2;
    let partition_count = 1;
    let replica_id = <porep::TreeHasher<TTree> as Hasher>::Domain::random(&mut rng);
    let data: Vec<u8> = (0..nodes)
        .flat_map(|_| fr_into_bytes(&Fr::random(&mut rng)))
        .collect();
    let mut mmapped_data = setup_replica(&data, &replica_path);

    // Preparation for SetupParameters.
    let setup_params: SetupParams<StackedDrg<'_, TTree, THasher>> =
        SetupParams::<StackedDrg<'_, TTree, THasher>> {
            vanilla_params: stacked::SetupParams {
                nodes,
                degree: BASE_DEGREE,
                expansion_degree: EXP_DEGREE,
                porep_id: utils::generate_random_id::<TTree>(&mut rng),
                challenges: Challenges::new_interactive(challenges),
                num_layers,
                api_version: ApiVersion::V1_2_0,
                api_features: vec![],
            },
            partitions: Some(partition_count),
            priority: false,
        };

    // Generate the PoRep proof, verifying-key and the public inputs.
    let pub_params = StackedCompound::setup(&setup_params).unwrap();
    let (proof, verifying_key, pub_inputs) = porep::generate_porep::<TTree, THasher>(
        &mut rng,
        &pub_params,
        &store_cfg,
        replica_id,
        &mut mmapped_data[..],
    )
    .unwrap();

    // The real publix input parameters look a bit different.
    println!("Converting outputs to pallet-compatible format... 💻");
    let pt_proof = pallet::into_pallet_proof(proof).unwrap();
    let pt_vkey = pallet::into_pallet_verifying_key(&verifying_key).unwrap();
    let pt_pub_inputs = pallet::into_pallet_public_inputs(&pub_inputs).unwrap();
    assert!(
        pt_pub_inputs.0.len() + 1 == pt_vkey.ic.len(),
        "Invalid VerifyingKey, num(pub-inputs) + 1 != num(vkey.ic) ❌"
    );

    let pt_vkey_bytes = pt_vkey.into_bytes();
    let pt_proof_bytes = pt_proof.into_bytes();
    let pt_inputs_bytes = pt_pub_inputs.into_bytes();

    // println!("Verifying conversion to pallet-compatible formats... 💻");
    // let pt_vkey = PtVerifyingKey::<Bls12>::from_bytes(&pt_vkey_bytes).unwrap();
    // let pt_pvk = PtPreparedVerifyingKey::<Bls12>::from(pt_vkey);
    // let pt_proof = PtProof::<Bls12>::from_bytes(&pt_proof_bytes).unwrap();
    // let pt_inputs = PtPublicInputs::<Bls12>::from_bytes(&pt_inputs_bytes).unwrap();
    // assert!(
    //     verify_proof(&pt_pvk, &pt_proof, &pt_inputs).is_ok(),
    //     "Could not verify proof via pallet implementation ❌"
    // );
    // println!("VerifyingKey, Proof and PublicInputs are pallet-convertible ✅");

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

    // stacked::clear_cache_dir(cache_dir.path()).expect("expect cache to be cleared");
    // cache_dir.close().unwrap();
}
