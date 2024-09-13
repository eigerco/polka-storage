//use std::{
//    collections::BTreeMap,
//    fs::File,
//    io::{Read, Seek, Write},
//    path::Path,
//};

//use anyhow::{ensure, Result};
//use blstrs::Scalar as Fr;
//use ff::Field;
//use filecoin_hashers::Hasher;
//use filecoin_proofs::{
//    add_piece, clear_cache, clear_synthetic_proofs, compute_comm_d, generate_piece_commitment,
//    generate_synth_proofs, generate_tree_c, generate_tree_r_last, generate_window_post,
//    get_seal_inputs, seal_commit_phase1, seal_commit_phase2, seal_commit_phase2_circuit_proofs,
//    seal_pre_commit_phase1, seal_pre_commit_phase2, unseal_range, validate_cache_for_commit,
//    verify_seal, Commitment, MerkleTreeTrait, PaddedBytesAmount, PieceInfo, PoRepConfig,
//    PoStConfig, PoStType, PrivateReplicaInfo, ProverId, SealCommitOutput, SealPreCommitOutput,
//    SealPreCommitPhase1Output, SectorShape2KiB, SectorSize, UnpaddedByteIndex, UnpaddedBytesAmount,
//};
//use merkletree::store::StoreConfig;
//use rand::{random, Rng, SeedableRng};
//use rand_xorshift::XorShiftRng;
//use sha2::{Digest, Sha256};
//use storage_proofs_core::{
//    api_version::{ApiFeature, ApiVersion},
//    cache_key::CacheKey,
//    merkle::get_base_tree_count,
//    sector::SectorId,
//};
//use tempfile::{tempdir, NamedTempFile, TempDir};

//const TEST_SEED: [u8; 16] = [
//    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
//];
//const SECTOR_SIZE_2_KIB: u64 = 1 << 11;

//mod parameters;

//fn main() {
//    println!("Generating a Winning PoSt proof...");
//    let mut rng = XorShiftRng::from_seed(TEST_SEED);

//    // Parameterisation.
//    let total_sector_count = 10;
//    let sector_count = 1;
//    let sector_size: u64 = SECTOR_SIZE_2_KIB;

//    // Configuration part.
//    let api_version = ApiVersion::V1_2_0;
//    let post_config = PoStConfig {
//        sector_size: SectorSize(sector_size),
//        challenge_count: 10,
//        sector_count,
//        typ: PoStType::Window,
//        priority: false,
//        api_version,
//    };

//    let prover_fr: <<SectorShape2KiB as MerkleTreeTrait>::Hasher as Hasher>::Domain =
//        Fr::random(&mut rng).into();
//    let mut prover_id = [0u8; 32];
//    prover_id.copy_from_slice(AsRef::<[u8]>::as_ref(&prover_fr));

//    let mut priv_replicas = BTreeMap::new();
//    let porep_config = PoRepConfig::new_groth16(sector_size, [129; 32], api_version);
//    for _ in 0..total_sector_count {
//        let (sector_id, replica, comm_r, cache_dir) =
//            create_seal::<_, SectorShape2KiB>(&porep_config, &mut rng, prover_id, true).unwrap();
//        priv_replicas.insert(
//            sector_id,
//            PrivateReplicaInfo::<SectorShape2KiB>::new(
//                replica.path().into(),
//                comm_r,
//                cache_dir.path().into(),
//            )
//            .unwrap(),
//        );
//    }

//    let random_fr: <<SectorShape2KiB as MerkleTreeTrait>::Hasher as Hasher>::Domain =
//        Fr::random(&mut rng).into();
//    let mut randomness = [0u8; 32];
//    randomness.copy_from_slice(AsRef::<[u8]>::as_ref(&random_fr));

//    // Finally, proof generation.
//    // let proof = generate_window_post_with_vanilla(&post_config, &randomness, prover_id, vanilla_proofs).unwrap();
//    let proof = generate_window_post(&post_config, &randomness, &priv_replicas, prover_id).unwrap();
//    assert_eq!(proof.len(), 384);

//    // Convert to hex for usage in polkadot.js
//    println!("Proof:\n{proof:?}");
//}

//fn create_seal<R: Rng, Tree: 'static + MerkleTreeTrait>(
//    porep_config: &PoRepConfig,
//    rng: &mut R,
//    prover_id: ProverId,
//    skip_proof: bool,
//) -> Result<(SectorId, NamedTempFile, Commitment, TempDir)> {
//    let (mut piece_file, piece_bytes) = generate_piece_file(porep_config.sector_size.into())?;
//    let sealed_sector_file = NamedTempFile::new()?;
//    let cache_dir = tempdir().expect("failed to create temp dir");

//    let ticket = rng.gen();
//    let seed = rng.gen();
//    let sector_id = rng.gen::<u64>().into();

//    let (piece_infos, phase1_output) = run_seal_pre_commit_phase1::<Tree>(
//        porep_config,
//        prover_id,
//        sector_id,
//        ticket,
//        &cache_dir,
//        &mut piece_file,
//        &sealed_sector_file,
//    )?;

//    let num_layers = phase1_output.labels.len();
//    let pre_commit_output = seal_pre_commit_phase2(
//        porep_config,
//        phase1_output,
//        cache_dir.path(),
//        sealed_sector_file.path(),
//    )?;

//    // Check if creating only the tree_r_last generates the same output as the full pre commit
//    // phase 2 process.
//    let tree_r_last_dir = tempdir().expect("failed to create temp dir");
//    generate_tree_r_last::<_, _, Tree>(
//        porep_config.sector_size.into(),
//        &sealed_sector_file,
//        &tree_r_last_dir,
//    )?;
//    compare_trees::<Tree>(&tree_r_last_dir, &cache_dir, CacheKey::CommRLastTree)?;

//    // Check if creating only the tree_r generates the same output as the full pre commit phase 2
//    // process.
//    let tree_c_dir = tempdir().expect("failed to create temp dir");
//    generate_tree_c::<_, _, Tree>(
//        porep_config.sector_size.into(),
//        &cache_dir,
//        &tree_c_dir,
//        num_layers,
//    )?;
//    compare_trees::<Tree>(&tree_c_dir, &cache_dir, CacheKey::CommCTree)?;

//    let comm_r = pre_commit_output.comm_r;

//    if skip_proof {
//        if porep_config.feature_enabled(ApiFeature::SyntheticPoRep) {
//            clear_synthetic_proofs::<Tree>(cache_dir.path())?;
//        }
//        clear_cache::<Tree>(cache_dir.path())?;
//    } else {
//        proof_and_unseal::<Tree>(
//            porep_config,
//            cache_dir.path(),
//            &sealed_sector_file,
//            prover_id,
//            sector_id,
//            ticket,
//            seed,
//            pre_commit_output,
//            &piece_infos,
//            &piece_bytes,
//        )
//        .expect("failed to proof_and_unseal");
//    }

//    Ok((sector_id, sealed_sector_file, comm_r, cache_dir))
//}

//fn generate_piece_file(sector_size: u64) -> Result<(NamedTempFile, Vec<u8>)> {
//    let number_of_bytes_in_piece = UnpaddedBytesAmount::from(PaddedBytesAmount(sector_size));

//    let piece_bytes: Vec<u8> = (0..number_of_bytes_in_piece.0)
//        .map(|_| random::<u8>())
//        .collect();

//    let mut piece_file = NamedTempFile::new()?;
//    piece_file.write_all(&piece_bytes)?;
//    piece_file.as_file_mut().sync_all()?;
//    piece_file.as_file_mut().rewind()?;

//    Ok((piece_file, piece_bytes))
//}

//fn compare_trees<Tree: 'static + MerkleTreeTrait>(
//    dir_a: &TempDir,
//    dir_b: &TempDir,
//    cache_key: CacheKey,
//) -> Result<()> {
//    let base_tree_count = get_base_tree_count::<Tree>();
//    let cache_key_names = if base_tree_count == 1 {
//        vec![cache_key.to_string()]
//    } else {
//        (0..base_tree_count)
//            .map(|count| format!("{}-{}", cache_key, count))
//            .collect()
//    };
//    for cache_key_name in cache_key_names {
//        let hash_a = hash_file(dir_a, &cache_key_name)?;
//        let hash_b = hash_file(dir_b, &cache_key_name)?;
//        assert_eq!(hash_a, hash_b, "files are identical");
//    }
//    Ok(())
//}

//fn hash_file(dir: &TempDir, cache_key: &str) -> Result<Vec<u8>> {
//    let path = StoreConfig::data_path(dir.path(), cache_key);
//    let mut hasher = Sha256::new();
//    let mut file = File::open(path)?;
//    std::io::copy(&mut file, &mut hasher)?;
//    Ok(hasher.finalize().to_vec())
//}

//#[allow(clippy::too_many_arguments)]
//fn proof_and_unseal<Tree: 'static + MerkleTreeTrait>(
//    config: &PoRepConfig,
//    cache_dir_path: &Path,
//    sealed_sector_file: &NamedTempFile,
//    prover_id: ProverId,
//    sector_id: SectorId,
//    ticket: [u8; 32],
//    seed: [u8; 32],
//    pre_commit_output: SealPreCommitOutput,
//    piece_infos: &[PieceInfo],
//    piece_bytes: &[u8],
//) -> Result<()> {
//    let aggregation_enabled = false;
//    let (commit_output, _commit_inputs, _seed, _comm_r) = generate_proof::<Tree>(
//        config,
//        cache_dir_path,
//        sealed_sector_file,
//        prover_id,
//        sector_id,
//        ticket,
//        seed,
//        &pre_commit_output,
//        piece_infos,
//        aggregation_enabled,
//    )?;

//    unseal::<Tree>(
//        config,
//        cache_dir_path,
//        sealed_sector_file,
//        prover_id,
//        sector_id,
//        ticket,
//        seed,
//        &pre_commit_output,
//        piece_infos,
//        piece_bytes,
//        &commit_output,
//    )
//}

//#[allow(clippy::too_many_arguments)]
//fn unseal<Tree: 'static + MerkleTreeTrait>(
//    config: &PoRepConfig,
//    cache_dir_path: &Path,
//    sealed_sector_file: &NamedTempFile,
//    prover_id: ProverId,
//    sector_id: SectorId,
//    ticket: [u8; 32],
//    seed: [u8; 32],
//    pre_commit_output: &SealPreCommitOutput,
//    piece_infos: &[PieceInfo],
//    piece_bytes: &[u8],
//    commit_output: &SealCommitOutput,
//) -> Result<()> {
//    let comm_d = pre_commit_output.comm_d;
//    let comm_r = pre_commit_output.comm_r;

//    let mut unseal_file = NamedTempFile::new()?;
//    let _ = unseal_range::<_, _, _, Tree>(
//        config,
//        cache_dir_path,
//        sealed_sector_file,
//        &unseal_file,
//        prover_id,
//        sector_id,
//        comm_d,
//        ticket,
//        UnpaddedByteIndex(508),
//        UnpaddedBytesAmount(508),
//    )?;

//    unseal_file.rewind()?;

//    let mut contents = vec![];
//    assert!(
//        unseal_file.read_to_end(&mut contents).is_ok(),
//        "failed to populate buffer with unsealed bytes"
//    );
//    assert_eq!(contents.len(), 508);
//    assert_eq!(&piece_bytes[508..508 + 508], &contents[..]);

//    let computed_comm_d = compute_comm_d(config.sector_size, piece_infos)?;

//    assert_eq!(
//        comm_d, computed_comm_d,
//        "Computed and expected comm_d don't match."
//    );

//    let verified = verify_seal::<Tree>(
//        config,
//        comm_r,
//        comm_d,
//        prover_id,
//        sector_id,
//        ticket,
//        seed,
//        &commit_output.proof,
//    )?;
//    assert!(verified, "failed to verify valid seal");
//    Ok(())
//}

//fn run_seal_pre_commit_phase1<Tree: 'static + MerkleTreeTrait>(
//    config: &PoRepConfig,
//    prover_id: ProverId,
//    sector_id: SectorId,
//    ticket: [u8; 32],
//    cache_dir: &TempDir,
//    mut piece_file: &mut NamedTempFile,
//    sealed_sector_file: &NamedTempFile,
//) -> Result<(Vec<PieceInfo>, SealPreCommitPhase1Output<Tree>)> {
//    let number_of_bytes_in_piece = config.unpadded_bytes_amount();

//    let piece_info = generate_piece_commitment(piece_file.as_file_mut(), number_of_bytes_in_piece)?;
//    piece_file.as_file_mut().rewind()?;

//    let mut staged_sector_file = NamedTempFile::new()?;
//    add_piece(
//        &mut piece_file,
//        &mut staged_sector_file,
//        number_of_bytes_in_piece,
//        &[],
//    )?;

//    let piece_infos = vec![piece_info];

//    let phase1_output = seal_pre_commit_phase1::<_, _, _, Tree>(
//        config,
//        cache_dir.path(),
//        staged_sector_file.path(),
//        sealed_sector_file.path(),
//        prover_id,
//        sector_id,
//        ticket,
//        &piece_infos,
//    )?;

//    // validate_cache_for_precommit_phase2(
//    //     cache_dir.path(),
//    //     staged_sector_file.path(),
//    //     &phase1_output,
//    // )?;

//    Ok((piece_infos, phase1_output))
//}

//#[allow(clippy::too_many_arguments)]
//fn generate_proof<Tree: 'static + MerkleTreeTrait>(
//    config: &PoRepConfig,
//    cache_dir_path: &Path,
//    sealed_sector_file: &NamedTempFile,
//    prover_id: ProverId,
//    sector_id: SectorId,
//    ticket: [u8; 32],
//    seed: [u8; 32],
//    pre_commit_output: &SealPreCommitOutput,
//    piece_infos: &[PieceInfo],
//    aggregation_enabled: bool,
//) -> Result<(SealCommitOutput, Vec<Vec<Fr>>, [u8; 32], [u8; 32])> {
//    if config.feature_enabled(ApiFeature::SyntheticPoRep) {
//        generate_synth_proofs::<_, Tree>(
//            config,
//            cache_dir_path,
//            sealed_sector_file.path(),
//            prover_id,
//            sector_id,
//            ticket,
//            pre_commit_output.clone(),
//            piece_infos,
//        )?;
//        clear_cache::<Tree>(cache_dir_path)?;
//    } else {
//        validate_cache_for_commit::<_, _, Tree>(cache_dir_path, sealed_sector_file.path())?;
//    }

//    let phase1_output = seal_commit_phase1::<_, Tree>(
//        config,
//        cache_dir_path,
//        sealed_sector_file.path(),
//        prover_id,
//        sector_id,
//        ticket,
//        seed,
//        pre_commit_output.clone(),
//        piece_infos,
//    )?;

//    if config.feature_enabled(ApiFeature::SyntheticPoRep) {
//        clear_synthetic_proofs::<Tree>(cache_dir_path)?;
//    } else {
//        clear_cache::<Tree>(cache_dir_path)?;
//    }

//    ensure!(
//        seed == phase1_output.seed,
//        "seed and phase1 output seed do not match"
//    );
//    ensure!(
//        ticket == phase1_output.ticket,
//        "seed and phase1 output ticket do not match"
//    );

//    let comm_r = phase1_output.comm_r;
//    let inputs = get_seal_inputs::<Tree>(
//        config,
//        phase1_output.comm_r,
//        phase1_output.comm_d,
//        prover_id,
//        sector_id,
//        phase1_output.ticket,
//        phase1_output.seed,
//    )?;

//    // This part of the test is demonstrating that if you want to use
//    // NI-PoRep AND aggregate the NI-PoRep proofs, you MUST generate
//    // the circuit proofs for each NI-PoRep proof, rather than the
//    // full seal commit proof.  If you are NOT aggregating multiple
//    // NI-PoRep proofs, you use the existing API as normal.
//    //
//    // The way the API is contructed, the generation is the ONLY
//    // difference in this case, as the aggregation and verification
//    // APIs remain the same.

//    let result = if config.feature_enabled(ApiFeature::NonInteractivePoRep) && aggregation_enabled {
//        seal_commit_phase2_circuit_proofs(config, phase1_output, sector_id)?
//    } else {
//        // We don't need to do anything special for aggregating
//        // InteractivePoRep seal proofs
//        seal_commit_phase2(config, phase1_output, prover_id, sector_id)?
//    };

//    Ok((result, inputs, seed, comm_r))
//}

//// fn create_fake_seal<R: rand::Rng, Tree: 'static + MerkleTreeTrait>(
////     mut rng: &mut R,
////     sector_size: u64,
////     porep_id: &[u8; 32],
////     api_version: ApiVersion,
//// ) -> Result<(SectorId, NamedTempFile, Commitment, TempDir)> {
////     let sealed_sector_file = NamedTempFile::new()?;

////     let config = porep_config(sector_size, *porep_id, api_version);

////     let cache_dir = tempdir().unwrap();

////     let sector_id = rng.gen::<u64>().into();

////     let comm_r = fauxrep_aux::<_, _, _, Tree>(
////         &mut rng,
////         &config,
////         cache_dir.path(),
////         sealed_sector_file.path(),
////     )?;

////     Ok((sector_id, sealed_sector_file, comm_r, cache_dir))
//// }
