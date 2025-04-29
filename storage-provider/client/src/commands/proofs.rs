use core::panic;
use std::{
    collections::HashSet,
    fmt::Display,
    io::Write,
    ops::Deref,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::ValueEnum;
use codec::Encode;
use itertools::Itertools;
use mater::CarV2ReaderExt;
use polka_storage_proofs::{
    match_post_proof, match_seal_proof,
    porep::{
        self,
        sealer::{create_sector, precommit_sector, prove_sector, PreCommitOutput},
    },
    post::{self, generate_window_post, ReplicaInfo},
    ZeroPaddingReader,
};
use polka_storage_provider_common::commp::{commp, CommPError};
use primitives::{
    commitment::{
        piece::{PaddedPieceSize, PieceInfo},
        CommD, CommP, CommR, Commitment, CommitmentError, CommitmentKind,
    },
    proofs::{derive_prover_id, RegisteredPoStProof, RegisteredSealProof},
    randomness::{draw_randomness, DomainSeparationTag},
    sector::SectorNumber,
    test_data::{
        absolute_block_number::Absolute, deal_timeline::DealTimeline,
        relative_block_number::Relative, sector_timeline::SectorTimeline,
    },
    MAX_SECTORS_PER_CALL,
};
use quote::format_ident;
use serde_json::json;
use storagext::multipair::{MultiPairArgs, MultiPairSigner};
use subxt::{
    ext::{
        sp_core::{sr25519, Pair},
        sp_runtime::AccountId32,
    },
    tx::{PairSigner, Signer},
};
use tempfile::tempdir;

use crate::CliError;

// Time is measured by number of blocks.
const MILLISECS_PER_BLOCK: u64 = 6000;
const MINUTES: u64 = 60_000 / (MILLISECS_PER_BLOCK as u64);

/// Utils sub-commands.
#[derive(Debug, clap::Subcommand)]
pub enum ProofsCommand {
    /// Calculate a piece commitment for the provided data stored at the a given path
    #[clap(alias = "commp")]
    CalculatePieceCommitment {
        /// Path to the data
        input_path: PathBuf,
    },
    /// Generates PoRep verifying key and proving parameters for zk-SNARK workflows (prove commit)
    #[clap(name = "porep-params")]
    GeneratePoRepParams {
        /// PoRep has multiple variants dependent on the sector size.
        /// Parameters are required for each sector size and its corresponding PoRep.
        #[arg(short, long, default_value = "2KiB")]
        seal_proof: RegisteredSealProof,
        /// Directory where the params files will be put. Defaults to the current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
    },
    /// DEMO COMMAND - Generates PoRep for a piece file.
    ///
    /// Takes a piece file (in a CARv2 archive, unpadded), puts it into a sector (temp file), seals and proves it.
    ///
    /// When you run the command for the first time on a clean `cache_directory` it will fail,
    /// because `rust-fil-proofs` tries to validate cache based on https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/parent_cache.json.
    ///
    /// When you run the command for the second time, the cache is recreated and there are no verification issues.
    #[clap(name = "porep")]
    PoRep {
        /// Key of the entity generating the proof.
        #[command(flatten)]
        signer_key: MultiPairArgs,
        /// PoRep has multiple variants dependent on the sector size.
        /// Parameters are required for each sector size and its corresponding PoRep Params.
        #[arg(short, long, default_value = "2KiB")]
        seal_proof: RegisteredSealProof,
        /// Path to where parameters to corresponding `seal_proof` are stored.
        #[arg(short, long)]
        proof_parameters_path: PathBuf,
        /// Directory where sector data like PersistentAux and TemporaryAux are stored.
        #[arg(short, long)]
        cache_directory: PathBuf,
        /// Piece file, CARv2 archive created with `mater-cli convert`.
        input_path: PathBuf,
        /// CommP of a file, calculated with `commp` command.
        commp: String,
        /// Directory where the proof files and the sector will be put. Defaults to the current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
        /// Sector number
        #[arg(long)]
        sector_id: u32,
        /// The height at which we draw the randomness for deriving a sealed cid.
        #[arg(long)]
        seal_randomness_height: u64,
        /// Precommit block number
        #[arg(long)]
        pre_commit_block_number: u64,
    },
    /// Generates PoSt verifying key and proving parameters for zk-SNARK workflows (submit windowed PoSt)
    #[clap(name = "post-params")]
    GeneratePoStParams {
        /// PoSt has multiple variants dependant on the sector size.
        /// Parameters are required for each sector size and its corresponding PoSt.
        #[arg(short, long, default_value = "2KiB")]
        post_type: RegisteredPoStProof,
        /// Directory where the params files will be put. Defaults to current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
    },
    /// Creates a PoSt for a single sector.
    #[clap(name = "post")]
    PoSt {
        /// Key of the entity generating the proof.
        #[command(flatten)]
        signer_key: MultiPairArgs,
        /// PoSt has multiple variants dependant on the sector size.
        /// Parameters are required for each sector size and its corresponding PoSt.
        #[arg(long, default_value = "2KiB")]
        post_type: RegisteredPoStProof,
        /// Path to where parameters to corresponding `post_type` are stored.
        #[arg(short, long)]
        proof_parameters_path: PathBuf,
        /// Directory where cache data from `porep` for the `replica_path` sector command has been stored.
        /// It must be the same, or else it won't work.
        #[arg(short, long)]
        cache_directory: PathBuf,
        #[arg(short, long)]
        /// Directory where the PoSt proof will be stored. Defaults to the current directory.
        output_path: Option<PathBuf>,
        /// Sector Number used in the PoRep command.
        #[arg(long)]
        sector_number: u32,
        /// Block Number at which the randomness should be fetched from.
        /// It comes from the [`pallet_storage_provider::DeadlineInfo::challenge`] field.
        #[arg(long)]
        challenge_block: u64,
        /// Replica file generated with `porep` command e.g. `77.sector.sealed`.
        replica_path: PathBuf,
        /// CID - CommR of a replica (output of `porep` command)
        comm_r: String,
    },
    /// Generates PoRep params, PoSt params, proofs and commitments used for benchmarking.
    #[clap(name = "benchmark-data")]
    BenchmarkData {
        /// Piece file, CARv2 archive created with `mater-cli convert`.
        input_path: PathBuf,
        /// PoRep and PoSt have multiple variants dependent on the sector size.
        /// Parameters are required for each sector size.
        #[arg(long)]
        sector_size: SectorSizeArg,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorSizeArg {
    #[clap(name = "2KiB")]
    _2KiB,
    #[clap(name = "8MiB")]
    _8MiB,
    #[clap(name = "512MiB")]
    _512MiB,
    #[clap(name = "1GiB")]
    _1GiB,
}

impl SectorSizeArg {
    fn porep(&self) -> RegisteredSealProof {
        match self {
            SectorSizeArg::_2KiB => RegisteredSealProof::StackedDRG2KiBV1P1,
            SectorSizeArg::_8MiB => RegisteredSealProof::StackedDRG8MiBV1,
            SectorSizeArg::_512MiB => RegisteredSealProof::StackedDRG512MiBV1,
            SectorSizeArg::_1GiB => RegisteredSealProof::StackedDRG1GiBV1,
        }
    }

    fn post(&self) -> RegisteredPoStProof {
        match self {
            SectorSizeArg::_2KiB => RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
            SectorSizeArg::_8MiB => RegisteredPoStProof::StackedDRGWindow8MiBV1,
            SectorSizeArg::_512MiB => RegisteredPoStProof::StackedDRGWindow512MiBV1,
            SectorSizeArg::_1GiB => RegisteredPoStProof::StackedDRGWindow1GiBV1,
        }
    }
}

impl Display for SectorSizeArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SectorSizeArg::_2KiB => write!(f, "2KiB"),
            SectorSizeArg::_8MiB => write!(f, "8MiB"),
            SectorSizeArg::_512MiB => write!(f, "512MiB"),
            SectorSizeArg::_1GiB => write!(f, "1GiB"),
        }
    }
}

const POREP_PARAMS_EXT: &str = "porep.params";
const POREP_VK_EXT: &str = "porep.vk";
const POREP_VK_EXT_SCALE: &str = "porep.vk.scale";

const POST_PARAMS_EXT: &str = "post.params";
const POST_VK_EXT: &str = "post.vk";
const POST_VK_EXT_SCALE: &str = "post.vk.scale";

const POREP_PROOF_EXT: &str = "sector.proof.porep.scale";
const POST_PROOF_EXT: &str = "sector.proof.post.scale";

const KEYS_DIR: &str = "test-fixtures/keys";
const PROOFS_DIR: &str = "test-fixtures/proofs";
const PARAMS_CACHE_DIR: &str = "target/params";
const BENCH_DATA_DIR_TO_ROOT: &str = "../../..";

impl ProofsCommand {
    /// Run the command.
    pub async fn run(self) -> Result<(), CliError> {
        match self {
            ProofsCommand::CalculatePieceCommitment { input_path } => {
                calculate_piece_commitment(input_path).await?;
            }
            ProofsCommand::GeneratePoRepParams {
                seal_proof,
                output_path,
            } => {
                generate_porep_params(output_path.clone(), output_path, seal_proof)?;
            }
            ProofsCommand::PoRep {
                signer_key,
                seal_proof,
                proof_parameters_path,
                input_path,
                commp,
                output_path,
                cache_directory,
                sector_id,
                seal_randomness_height,
                pre_commit_block_number,
            } => {
                let Some(signer) = Option::<MultiPairSigner>::from(signer_key) else {
                    return Err(UtilsCommandError::NoSigner)?;
                };
                porep(
                    &signer,
                    sector_id,
                    seal_randomness_height,
                    pre_commit_block_number,
                    output_path,
                    input_path,
                    proof_parameters_path,
                    commp,
                    seal_proof,
                    &cache_directory,
                )
                .await?;
            }
            ProofsCommand::GeneratePoStParams {
                post_type,
                output_path,
            } => {
                generate_post_params(output_path, post_type)?;
            }
            ProofsCommand::PoSt {
                signer_key,
                post_type,
                proof_parameters_path,
                cache_directory,
                replica_path,
                comm_r,
                output_path,
                sector_number,
                challenge_block,
            } => {
                let Some(signer) = Option::<MultiPairSigner>::from(signer_key) else {
                    return Err(UtilsCommandError::NoSigner)?;
                };
                post(
                    &signer,
                    challenge_block,
                    output_path,
                    sector_number,
                    comm_r,
                    replica_path,
                    cache_directory,
                    proof_parameters_path,
                    post_type,
                )?;
            }
            ProofsCommand::BenchmarkData {
                input_path,
                sector_size,
            } => {
                benchmark_data(input_path, sector_size).await?;
            }
        }

        Ok(())
    }
}

async fn calculate_piece_commitment(
    input_path: impl AsRef<Path>,
) -> Result<(Commitment<CommP>, PaddedPieceSize), CliError> {
    let input_path = input_path.as_ref();
    // Check if the file is a CARv2 file. If it is, we can't calculate the piece commitment.
    let mut source_file = tokio::fs::File::open(&input_path).await?;
    source_file
        .is_car_file()
        .await
        .map_err(|e| UtilsCommandError::InvalidCARv2(input_path.to_owned(), e))?;
    let (commitment, padded_piece_size) =
        commp(&input_path).map_err(|err| UtilsCommandError::CommPError(err))?;
    let cid = commitment.cid();

    println!(
        "{:#}",
        json!({
            "cid": cid.to_string(),
            "size": padded_piece_size
        })
    );

    Ok((commitment, padded_piece_size))
}

fn generate_porep_params(
    params_output_path: Option<PathBuf>,
    keys_output_path: Option<PathBuf>,
    seal_proof: RegisteredSealProof,
) -> Result<(), CliError> {
    let current_dir = std::env::current_dir()?;
    let params_output_path = params_output_path.unwrap_or_else(|| current_dir.clone());
    let keys_output_path = keys_output_path.unwrap_or_else(move || current_dir);
    let file_name: String = seal_proof.sector_size().to_string();
    let (parameters_file_name, mut parameters_file) =
        file_with_extension(params_output_path, file_name.as_str(), POREP_PARAMS_EXT)?;
    let (vk_file_name, mut vk_file) =
        file_with_extension(keys_output_path.clone(), file_name.as_str(), POREP_VK_EXT)?;
    let (vk_scale_file_name, mut vk_scale_file) =
        file_with_extension(keys_output_path, file_name.as_str(), POREP_VK_EXT_SCALE)?;
    println!(
        "Generating params for {} sectors... It can take a couple of minutes ⌛",
        file_name
    );
    let parameters = porep::generate_random_groth16_parameters(seal_proof)
        .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;
    parameters.write(&mut parameters_file)?;
    parameters.vk.write(&mut vk_file)?;
    let vk =
        polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(parameters.vk.clone())
            .map_err(|e| UtilsCommandError::FromBytesError(e))?;
    let bytes = codec::Encode::encode(&vk);
    vk_scale_file.write_all(&bytes)?;
    println!("Generated parameters: ");
    println!("{}", parameters_file_name.display());
    println!("{}", vk_file_name.display());
    println!("{}", vk_scale_file_name.display());
    Ok(())
}

async fn porep(
    signer: &MultiPairSigner,
    sector_id: u32,
    seal_randomness_height: u64,
    pre_commit_block_number: u64,
    output_path: Option<PathBuf>,
    input_path: PathBuf,
    proof_parameters_path: PathBuf,
    commp: String,
    seal_proof: RegisteredSealProof,
    cache_directory: impl AsRef<Path>,
) -> Result<PreCommitOutput, CliError> {
    let sector_number =
        SectorNumber::try_from(sector_id).map_err(|_| UtilsCommandError::InvalidSectorId)?;
    let entropy = signer.account_id().encode();
    println!("Entropy: {}", hex::encode(&entropy));
    let ticket = get_randomness(
        DomainSeparationTag::SealRandomness,
        seal_randomness_height,
        &entropy,
    );
    println!(
        "[{seal_randomness_height}] Ticket randomness: {}",
        hex::encode(ticket)
    );

    // The number added is configured in runtime:
    // https://github.com/eigerco/polka-storage/blob/18207759d7c6c175916d5bed70246d94a8f028f4/runtime/src/configs/mod.rs#L360
    let interactive_block_number = pre_commit_block_number + 10;
    let seed = get_randomness(
        DomainSeparationTag::InteractiveSealChallengeSeed,
        interactive_block_number,
        &entropy,
    );
    println!(
        "[{interactive_block_number}] Seed randomness: {}",
        hex::encode(seed)
    );
    let output_path = if let Some(output_path) = output_path {
        output_path
    } else {
        std::env::current_dir()?
    };
    let (proof_scale_filename, proof_scale_file) = file_with_extension(
        output_path.clone(),
        format!("{}", sector_id).as_str(),
        POREP_PROOF_EXT,
    )?;
    let mut source_file = tokio::fs::File::open(&input_path).await?;
    source_file
        .is_car_file()
        .await
        .map_err(|e| UtilsCommandError::InvalidCARv2(input_path.to_owned(), e))?;
    let proof_parameters = porep::load_groth16_parameters(proof_parameters_path)
        .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;
    let piece_file = std::fs::File::open(&input_path)
        .map_err(|e| UtilsCommandError::InvalidPieceFile(input_path.to_owned(), e))?;
    let piece_file_length = piece_file
        .metadata()
        .map_err(|e| UtilsCommandError::InvalidPieceFile(input_path.to_owned(), e))?
        .len();
    let piece_file_length = PaddedPieceSize::from_arbitrary_size(piece_file_length);
    let piece_file = ZeroPaddingReader::new(piece_file, *piece_file_length.unpadded());
    let commp =
        cid::Cid::from_str(&commp).map_err(|e| UtilsCommandError::InvalidPieceCommP(commp, e))?;
    let piece_info = PieceInfo {
        commitment: Commitment::try_from(commp)
            .map_err(|e| UtilsCommandError::InvalidPieceType(commp.to_string(), e))?,
        size: piece_file_length,
    };
    let (unsealed_sector_path, unsealed_sector) = file_with_extension(
        output_path.clone(),
        format!("{}", sector_id).as_str(),
        "sector.unsealed",
    )?;
    let (sealed_sector_path, _) = file_with_extension(
        output_path,
        format!("{}", sector_id).as_str(),
        "sector.sealed",
    )?;
    println!("Creating sector...");
    let piece_infos = create_sector(seal_proof, vec![(piece_file, piece_info)], unsealed_sector)
        .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;
    let prover_id = derive_prover_id(signer.account_id());
    println!("Prover ID: {}", hex::encode(prover_id));
    println!("Precommitting...");
    let precommit = match_seal_proof!(
        seal_proof,
        precommit_sector::<_, _, _, _>(
            seal_proof,
            &cache_directory,
            unsealed_sector_path,
            &sealed_sector_path,
            prover_id,
            sector_number,
            ticket,
            &piece_infos
        )
    )
    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;
    println!("Proving...");
    let proofs = match_seal_proof!(
        seal_proof,
        prove_sector::<_, _, _>(
            seal_proof,
            &proof_parameters,
            &cache_directory,
            &sealed_sector_path,
            prover_id,
            sector_number,
            ticket,
            Some(seed),
            precommit,
            &piece_infos
        )
    )
    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

    println!(
        "[{seal_randomness_height}] Ticket randomness: {}",
        hex::encode(ticket)
    );
    println!(
        "[{interactive_block_number}] Seed randomness: {}",
        hex::encode(seed)
    );
    println!("CommD: {}", precommit.comm_d.cid());
    println!("CommR: {}", precommit.comm_r.cid());
    write_proof_file(proofs, proof_scale_file)?;

    println!("Wrote proof to {}", proof_scale_filename.display());
    Ok(precommit)
}

fn generate_post_params(
    output_path: Option<PathBuf>,
    post_type: RegisteredPoStProof,
) -> Result<(), CliError> {
    let output_path = if let Some(output_path) = output_path {
        output_path
    } else {
        std::env::current_dir()?
    };
    let file_name: String = post_type.sector_size().to_string();
    let (parameters_file_name, mut parameters_file) =
        file_with_extension(output_path.clone(), file_name.as_str(), POST_PARAMS_EXT)?;
    let (vk_file_name, mut vk_file) =
        file_with_extension(output_path.clone(), file_name.as_str(), POST_VK_EXT)?;
    let (vk_scale_file_name, mut vk_scale_file) =
        file_with_extension(output_path, file_name.as_str(), POST_VK_EXT_SCALE)?;
    println!(
        "Generating PoSt params for {} sectors... It can take a few secs ⌛",
        file_name
    );
    let parameters = post::generate_random_groth16_parameters(post_type)
        .map_err(|e| UtilsCommandError::GeneratePoStError(e))?;
    parameters.write(&mut parameters_file)?;
    parameters.vk.write(&mut vk_file)?;
    let vk = polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(parameters.vk)
        .map_err(|e| UtilsCommandError::FromBytesError(e))?;
    let bytes = codec::Encode::encode(&vk);
    vk_scale_file.write_all(&bytes)?;
    println!("Generated parameters: ");
    println!("{}", parameters_file_name.display());
    println!("{}", vk_file_name.display());
    println!("{}", vk_scale_file_name.display());
    Ok(())
}

fn post(
    signer: &MultiPairSigner,
    challenge_block: u64,
    output_path: Option<PathBuf>,
    sector_number: u32,
    comm_r: String,
    replica_path: PathBuf,
    cache_directory: impl AsRef<Path>,
    proof_parameters_path: PathBuf,
    post_type: RegisteredPoStProof,
) -> Result<(), CliError> {
    let entropy = signer.account_id().encode();
    let randomness = get_randomness(
        DomainSeparationTag::WindowedPoStChallengeSeed,
        challenge_block,
        &entropy,
    );
    let output_path = if let Some(output_path) = output_path {
        output_path
    } else {
        std::env::current_dir()?
    };
    let (proof_scale_filename, proof_scale_file) = file_with_extension(
        output_path,
        format!("{}", sector_number).as_str(),
        POST_PROOF_EXT,
    )?;
    let comm_r = cid::Cid::from_str(&comm_r).map_err(|_| UtilsCommandError::CommRError)?;
    let sector_number =
        SectorNumber::try_from(sector_number).map_err(|_| UtilsCommandError::InvalidSectorId)?;
    let replicas = vec![ReplicaInfo {
        sector_id: sector_number,
        comm_r: comm_r
            .hash()
            .digest()
            .try_into()
            .map_err(|_| UtilsCommandError::CommRError)?,
        replica_path,
        cache_path: cache_directory.as_ref().to_path_buf(),
    }];
    println!("Loading parameters...");
    let proof_parameters = post::load_groth16_parameters(proof_parameters_path)
        .map_err(|e| UtilsCommandError::GeneratePoStError(e))?;
    let prover_id = derive_prover_id(signer.account_id());
    let proofs = match_post_proof!(
        post_type,
        generate_window_post::<_>(
            post_type,
            &proof_parameters,
            randomness,
            prover_id,
            replicas
        )
    )
    .map_err(|e| UtilsCommandError::GeneratePoStError(e))?;

    println!("Proving...");
    write_proof_file(proofs, proof_scale_file)?;
    println!("Wrote proof to {}", proof_scale_filename.display());
    println!(
        "[{challenge_block}] Randomness: {}",
        hex::encode(randomness)
    );
    Ok(())
}

/// Converts multiple rust-fil-proofs to polka-storage-proofs, encodes them to a fixed length array,
/// and writes those arrays sequentially to a single file.
fn write_proof_file(
    proofs: Vec<bellperson::groth16::Proof<blstrs::Bls12>>,
    mut proof_scale_file: std::fs::File,
) -> Result<(), CliError> {
    let scale_encoded_proofs = proofs
        .into_iter()
        .flat_map(|proof| {
            codec::Encode::encode(
                &polka_storage_proofs::Proof::<bls12_381::Bls12>::try_from(proof)
                    .expect("conversion between rust-fil-proofs and polka-storage-proofs to work"),
            )
        })
        .collect::<Vec<_>>();
    proof_scale_file.write_all(&scale_encoded_proofs)?;
    Ok(())
}

/// This is a temporary representation of the data that ends up in a separate `BenchmarkData` struct
/// in `primitives/src/test_data/benchmark_data.rs`
#[derive(Debug)]
pub struct BenchmarkData {
    pub storage_provider_name: String,
    pub porep_verifying_key_path: PathBuf,
    pub post_verifying_key_path: PathBuf,
    pub seal_proof: RegisteredSealProof,
    pub post_type: RegisteredPoStProof,
    pub comm_p: Commitment<CommP>,
    pub sectors: Vec<SectorData>,
    pub timeline: SectorTimeline<u64, AccountId32>,
}

/// This is a temporary representation of the data that ends up in a separate `SectorData` struct
/// in `primitives/src/test_data/sector_data.rs`
#[derive(Debug)]
pub struct SectorData {
    pub sector_number: SectorNumber,
    pub padded_piece_size: PaddedPieceSize,
    pub comm_r: Commitment<CommR>,
    pub comm_d: Commitment<CommD>,
    pub porep_proof_path: PathBuf,
    pub post_proof_path: PathBuf,
}

async fn benchmark_data(input_path: PathBuf, sector_size: SectorSizeArg) -> Result<(), CliError> {
    const PROVIDER_NAME: &str = "//StorageProvider";
    let signer_key: MultiPairSigner = MultiPairSigner::Sr25519(PairSigner::new(
        sr25519::Pair::from_string(PROVIDER_NAME, None).expect("hardcoded key to be valid"),
    ));

    let proving_period = Relative::from(6 * MINUTES);
    let mut deal_start = Absolute::zero() + proving_period * 2;
    let mut quickest_post: Option<(Absolute<u64>, SectorTimeline<u64, _>)> = None;
    let mut errors: HashSet<String> = HashSet::new();
    // This loop will try to find a set of parameters that:
    // 1. Make up a valid timeline.
    // 2. Have the shortest block time before PoSt submission.
    for _ in 0..100 {
        // These are *somewhat* arbitrary but must fit conditions checked by SectorTimeline.
        let register_storage_provider = 1;
        let publish_storage_deals = 5;
        let pre_commit_sectors = 100;
        // The constants here (and `proving_period` above) are sourced from the
        // Testnet runtime defined in <runtime/src/configs/mod.rs>.
        let timeline = SectorTimeline::<u64, _>::new(
            signer_key.account_id(),
            register_storage_provider.into(),
            // 5 is arbitrary.
            publish_storage_deals.into(),
            // 10 is arbitrary.
            pre_commit_sectors.into(),
            // Using the minimum deal duration.
            vec![DealTimeline::new(deal_start.into(), (5 * MINUTES).into())],
            3,
            proving_period,
            (2 * MINUTES).into(),
            (1 * MINUTES).into(),
            (1 * MINUTES).into(),
        );
        let timeline = match timeline {
            Ok(timeline) => timeline,
            Err(e) => {
                errors.insert(e);
                continue;
            }
        };
        if timeline.deadline_index() == 0 {
            let current = timeline.submit_windowed_post();
            if let Some((prev, _)) = quickest_post {
                if current < prev {
                    quickest_post = Some((current, timeline));
                }
            } else {
                quickest_post = Some((current, timeline));
            }
        }
        deal_start = deal_start + proving_period;
    }
    let timeline = quickest_post
        .unwrap_or_else(|| panic!("no valid proof timeline found, errors: {:#?}", errors))
        .1;

    let seal_proof = sector_size.porep();
    let post_type = sector_size.post();

    println!(
        "--- Calculating piece commitment for {} ---",
        input_path.display()
    );
    let (comm_p, padded_piece_size) = calculate_piece_commitment(&input_path).await?;

    let params_root = PathBuf::from(PARAMS_CACHE_DIR);
    let keys_root = &PathBuf::from(KEYS_DIR);

    let porep_params_path = params_root.join(format!("{sector_size}.{POREP_PARAMS_EXT}"));
    let porep_params_vk_path = keys_root.join(format!("{sector_size}.{POREP_VK_EXT_SCALE}"));
    if !tokio::fs::try_exists(&porep_params_path).await?
        || !tokio::fs::try_exists(&porep_params_vk_path).await?
    {
        return Err(CliError::MissingPoRepParams { seal_proof });
    }
    println!("--- Using pre-generated PoRep params for seal proof {seal_proof:?} ---");
    println!("{}", porep_params_path.display());

    let post_params_path = params_root.join(format!("{sector_size}.{POST_PARAMS_EXT}"));
    let post_params_vk_path = keys_root.join(format!("{sector_size}.{POST_VK_EXT_SCALE}"));
    if !tokio::fs::try_exists(&post_params_path).await?
        || !tokio::fs::try_exists(&post_params_vk_path).await?
    {
        return Err(CliError::MissingPoStParams { post_type });
    }
    println!("--- Using pre-generated PoSt params for seal proof {seal_proof:?} ---");
    println!("{}", post_params_path.display());

    let mut benchmark_data = BenchmarkData {
        storage_provider_name: PROVIDER_NAME.to_owned(),
        porep_verifying_key_path: porep_params_vk_path,
        post_verifying_key_path: post_params_vk_path,
        seal_proof,
        post_type,
        comm_p,
        sectors: Vec::with_capacity(MAX_SECTORS_PER_CALL as usize),
        timeline: timeline.clone(),
    };

    let proofs_root = PathBuf::from(PROOFS_DIR).join(sector_size.to_string());
    tokio::fs::create_dir_all(&proofs_root).await?;

    for sector_number in 0..MAX_SECTORS_PER_CALL {
        println!("--- Sealing sector {sector_number} ---");
        let output_path = tempdir()?;
        tokio::fs::create_dir_all(&output_path).await?;
        let cache_directory = tempdir()?;
        tokio::fs::create_dir_all(&cache_directory).await?;

        let PreCommitOutput { comm_r, comm_d } = porep(
            &signer_key,
            sector_number,
            timeline.seal_randomness_height().0,
            timeline.pre_commit_sectors().0,
            Some(output_path.path().to_path_buf()),
            input_path.clone(),
            porep_params_path.clone(),
            comm_p.to_string(),
            seal_proof,
            &cache_directory,
        )
        .await?;

        let porep_file_name = format!("{sector_number}.{POREP_PROOF_EXT}");
        let porep_proof_path = proofs_root.join(&porep_file_name);
        tokio::fs::copy(output_path.path().join(&porep_file_name), &porep_proof_path).await?;

        println!("--- Creating PoSt proof for sector {sector_number} ---");
        let sealed_sector_path = output_path.path().join(format!("{}.sector.sealed", sector_number));
        post(
            &signer_key,
            timeline.deadline_challenge_block().0,
            Some(output_path.path().to_path_buf()),
            sector_number,
            comm_r.to_string(),
            sealed_sector_path,
            &cache_directory,
            post_params_path.clone(),
            post_type,
        )?;

        let post_file_name = format!("{sector_number}.{POST_PROOF_EXT}");
        let post_proof_path = proofs_root.join(&post_file_name);
        tokio::fs::copy(output_path.path().join(&post_file_name), &post_proof_path).await?;

        benchmark_data.sectors.push(SectorData {
            sector_number: SectorNumber::new(sector_number)
                .expect("sector IDs <= MAX_SECTORS_PER_CALL are safe"),
            padded_piece_size,
            comm_r,
            comm_d,
            porep_proof_path,
            post_proof_path,
        });
        drop(output_path);
        drop(cache_directory);
    }
    println!("--- Benchmarking data generated ---");
    let constructor = emit_benchmark_constructor(&benchmark_data);
    println!("{constructor}");
    Ok(())
}

fn emit_benchmark_constructor(benchmark_data: &BenchmarkData) -> String {
    let command = std::env::args().join(" ");
    let BenchmarkData {
        storage_provider_name,
        porep_verifying_key_path,
        post_verifying_key_path,
        seal_proof,
        post_type,
        comm_p,
        sectors,
        timeline,
    } = benchmark_data;

    let sectors = sectors
        .iter()
        .map(|sector| {
            let SectorData {
                sector_number,
                padded_piece_size,
                comm_r,
                comm_d,
                porep_proof_path,
                post_proof_path,
            } = sector;
            let sector_number = u32::from(*sector_number);
            let padded_piece_size = padded_piece_size.deref();
            let comm_r = emit_commitment(comm_r);
            let comm_d = emit_commitment(comm_d);
            let porep_proof_path_rel = PathBuf::from(BENCH_DATA_DIR_TO_ROOT).join(porep_proof_path);
            let porep_proof_path = porep_proof_path_rel.to_string_lossy();
            let post_proof_path_rel = PathBuf::from(BENCH_DATA_DIR_TO_ROOT).join(post_proof_path);
            let post_proof_path = post_proof_path_rel.to_string_lossy();
            quote::quote! {
                SectorData {
                    sector_number: SectorNumber::new(#sector_number).expect("valid sector ID"),
                    padded_piece_size: PaddedPieceSize::new(#padded_piece_size).expect("valid padded piece size"),
                    comm_r: #comm_r,
                    comm_d: #comm_d,
                    porep_proof: include_bytes!(#porep_proof_path),
                    post_proof: include_bytes!(#post_proof_path),
                }
            }
        })
        .collect::<Vec<_>>();

    let porep_verifying_key_path_rel =
        PathBuf::from(BENCH_DATA_DIR_TO_ROOT).join(porep_verifying_key_path);
    let porep_verifying_key_path = porep_verifying_key_path_rel.to_string_lossy();
    let post_verifying_key_path_rel =
        PathBuf::from(BENCH_DATA_DIR_TO_ROOT).join(post_verifying_key_path);
    let post_verifying_key_path = post_verifying_key_path_rel.to_string_lossy();
    let seal_proof = format_ident!("{seal_proof:?}");
    let post_type = format_ident!("{post_type:?}");
    let comm_p = emit_commitment(comm_p);
    let timeline = emit_timeline(timeline);
    let code = quote::quote! {
        BenchmarkData {
            storage_provider_name: #storage_provider_name,
            porep_verifying_key: include_bytes!(#porep_verifying_key_path),
            post_verifying_key: include_bytes!(#post_verifying_key_path),
            seal_proof: RegisteredSealProof::#seal_proof,
            post_type: RegisteredPoStProof::#post_type,
            comm_p: #comm_p,
            sectors: vec![#(#sectors),*],
            timeline: #timeline,
            _phantom: PhantomData,
        }
    };

    format!(
        "
        // DO NOT MODIFY
        // This code has been generated by `{command}`
        {code}
        "
    )
}

fn emit_commitment<Kind: CommitmentKind>(
    commitment: &Commitment<Kind>,
) -> proc_macro2::TokenStream {
    let commitment = commitment.cid().to_string();
    // Hacky, but works.
    let ty = std::any::type_name::<Kind>()
        .rsplit_once("::")
        .expect("valid path")
        .1;
    let kind = format_ident!("{ty}");
    quote::quote! {
        Commitment::<#kind>::from_cid(
            &Cid::from_str(#commitment).expect("valid cid"),
        ).expect("valid commitment")
    }
}

fn emit_timeline(timeline: &SectorTimeline<u64, AccountId32>) -> proc_macro2::TokenStream {
    let storage_provider_account_id: [u8; 32] =
        timeline.storage_provider_account_id().clone().into();
    let register_storage_provider = timeline.register_storage_provider().0;
    let publish_storage_deals = timeline.publish_storage_deals().0;
    let pre_commit_sectors = timeline.pre_commit_sectors().0;
    let deals = timeline.deals().iter().map(|t| {
        let start = t.start().0;
        let duration = t.duration().0;
        quote::quote! {
            DealTimeline::new(
                Absolute::from(BlockNumberFor::<T>::from(#start)),
                Relative::from(BlockNumberFor::<T>::from(#duration)),
            )
        }
    });
    let period_deadlines = timeline.period_deadlines();
    let proving_period = timeline.proving_period().0;
    let challenge_window = timeline.challenge_window().0;
    let challenge_lookback = timeline.challenge_lookback().0;
    let pre_commit_challenge_delay = timeline.pre_commit_challenge_delay().0;

    let proving_period_offset = timeline.proving_period_offset().0;
    let proving_period_start_initial = timeline.proving_period_start_initial().0;
    let seal_randomness_height = timeline.seal_randomness_height().0;
    let sector_expiration = timeline.sector_expiration().0;
    let interactive_block_number = timeline.interactive_block_number().0;
    let prove_commit_sectors = timeline.prove_commit_sectors().0;
    let deadline_index = timeline.deadline_index();
    let deadline_challenge_block = timeline.deadline_challenge_block().0;
    let deadline_start = timeline.deadline_start().0;
    let submit_windowed_post = timeline.submit_windowed_post().0;
    let deadline_close = timeline.deadline_close().0;
    quote::quote! {
        {
            let _proving_period_offset = #proving_period_offset;
            let _proving_period_start_initial = #proving_period_start_initial;
            let _seal_randomness_height = #seal_randomness_height;
            let _sector_expiration = #sector_expiration;
            let _interactive_block_number = #interactive_block_number;
            let _prove_commit_sectors = #prove_commit_sectors;
            let _deadline_index = #deadline_index;
            let _deadline_challenge_block = #deadline_challenge_block;
            let _deadline_start = #deadline_start;
            let _submit_windowed_post = #submit_windowed_post;
            let _deadline_close = #deadline_close;
            SectorTimeline::new(
                AccountId32::new([#(#storage_provider_account_id),*]),
                Absolute::from(BlockNumberFor::<T>::from(#register_storage_provider)),
                Absolute::from(BlockNumberFor::<T>::from(#publish_storage_deals)),
                Absolute::from(BlockNumberFor::<T>::from(#pre_commit_sectors)),
                vec![#(#deals),*],
                #period_deadlines,
                Relative::from(BlockNumberFor::<T>::from(#proving_period)),
                Relative::from(BlockNumberFor::<T>::from(#challenge_window)),
                Relative::from(BlockNumberFor::<T>::from(#pre_commit_challenge_delay)),
                Relative::from(BlockNumberFor::<T>::from(#challenge_lookback)),
            ).expect("valid timeline")
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UtilsCommandError {
    #[error("the commp command failed because: {0}")]
    CommPError(#[from] CommPError),
    #[error("failed to create a file '{0}' because: {1}")]
    FileCreateError(PathBuf, std::io::Error),
    #[error("failed to convert from rust-fil-proofs to polka-storage-proofs: {0}")]
    FromBytesError(#[from] polka_storage_proofs::FromBytesError),
    #[error("failed to generate a porep: {0}")]
    GeneratePoRepError(#[from] porep::PoRepError),
    #[error("failed to generate a post: {0}")]
    GeneratePoStError(#[from] post::PoStError),
    #[error("CommR must be 32 bytes and generated by `po-rep` command")]
    CommRError,
    #[error("failed to load piece file at path: {0}")]
    InvalidPieceFile(PathBuf, std::io::Error),
    #[error("provided invalid CommP {0}, error: {1}")]
    InvalidPieceCommP(String, cid::Error),
    #[error("invalid piece type, error: {1}")]
    InvalidPieceType(String, CommitmentError),
    #[error("invalid sector id")]
    InvalidSectorId,
    #[error("file {0} is invalid CARv2 file {1}")]
    InvalidCARv2(PathBuf, mater::Error),
    #[error("no signer key was provider")]
    NoSigner,
    #[error("error when serializing to json: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("error when decoding from scale file: {0}")]
    ParityScale(#[from] codec::Error),
}

fn file_with_extension(
    mut output_path: PathBuf,
    file_name: &str,
    extension: &str,
) -> Result<(PathBuf, std::fs::File), UtilsCommandError> {
    output_path.push(file_name);
    output_path.set_extension(extension);

    let file = std::fs::File::create(&output_path)
        .map_err(|e| UtilsCommandError::FileCreateError(output_path.clone(), e))?;
    Ok((output_path, file))
}

fn get_randomness(
    personalization: DomainSeparationTag,
    block_number: u64,
    entropy: &[u8],
) -> [u8; 32] {
    // This randomness digest is hardcoded because it's always same on testnet.
    let digest = [0u8; 32];
    draw_randomness(&digest, personalization, block_number, &entropy)
}
