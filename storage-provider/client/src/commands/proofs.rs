use std::{
    fs::File,
    io::{BufReader, Write},
    path::PathBuf,
    str::FromStr,
};

use codec::Encode;
use mater::CarV2Reader;
use polka_storage_proofs::{
    porep::{self, sealer::Sealer},
    post::{self, ReplicaInfo},
    ZeroPaddingReader,
};
use polka_storage_provider_common::commp::{calculate_piece_commitment, CommPError};
use primitives::{
    commitment::{
        piece::{PaddedPieceSize, PieceInfo},
        Commitment, CommitmentError,
    },
    proofs::{derive_prover_id, RegisteredPoStProof, RegisteredSealProof},
    randomness::{draw_randomness, DomainSeparationTag},
    sector::SectorNumber,
};
use storagext::multipair::{MultiPairArgs, MultiPairSigner};
use subxt::tx::Signer;

use crate::CliError;

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
}

const POREP_PARAMS_EXT: &str = "porep.params";
const POREP_VK_EXT: &str = "porep.vk";
const POREP_VK_EXT_SCALE: &str = "porep.vk.scale";

const POST_PARAMS_EXT: &str = "post.params";
const POST_VK_EXT: &str = "post.vk";
const POST_VK_EXT_SCALE: &str = "post.vk.scale";

const POREP_PROOF_EXT: &str = "sector.proof.porep.scale";
const POST_PROOF_EXT: &str = "sector.proof.post.scale";

impl ProofsCommand {
    /// Run the command.
    pub async fn run(self) -> Result<(), CliError> {
        match self {
            ProofsCommand::CalculatePieceCommitment { input_path } => {
                // Check if the file is a CARv2 file. If it is, we can't calculate the piece commitment.
                let mut source_file = tokio::fs::File::open(&input_path).await?;
                let mut car_v2_reader = CarV2Reader::new(&mut source_file);
                car_v2_reader
                    .is_car_file()
                    .await
                    .map_err(|e| UtilsCommandError::InvalidCARv2(input_path.clone(), e))?;

                // Calculate the piece commitment.
                let source_file = File::open(&input_path)?;
                let file_size = source_file.metadata()?.len();

                let buffered = BufReader::new(source_file);
                let padded_piece_size = PaddedPieceSize::from_arbitrary_size(file_size as u64);
                let mut zero_padding_reader = ZeroPaddingReader::new(buffered, *padded_piece_size);

                // The calculate_piece_commitment blocks the thread. We could
                // use tokio::task::spawn_blocking to avoid this, but in this
                // case it doesn't matter because this is the only thing we are
                // working on.
                let commitment =
                    calculate_piece_commitment(&mut zero_padding_reader, padded_piece_size)
                        .map_err(|err| UtilsCommandError::CommPError(err))?;
                let cid = commitment.cid();

                // NOTE(@jmg-duarte,09/10/2024): too lazy for proper json
                // plus adding an extra structure for such a small thing seems wasteful
                println!("{{\n\t\"cid\": \"{cid}\",\n\t\"size\": {padded_piece_size}\n}}");
            }
            ProofsCommand::GeneratePoRepParams {
                seal_proof,
                output_path,
            } => {
                let output_path = if let Some(output_path) = output_path {
                    output_path
                } else {
                    std::env::current_dir()?
                };

                let file_name: String = seal_proof.sector_size().to_string();

                let (parameters_file_name, mut parameters_file) =
                    file_with_extension(&output_path, file_name.as_str(), POREP_PARAMS_EXT)?;
                let (vk_file_name, mut vk_file) =
                    file_with_extension(&output_path, file_name.as_str(), POREP_VK_EXT)?;
                let (vk_scale_file_name, mut vk_scale_file) =
                    file_with_extension(&output_path, file_name.as_str(), POREP_VK_EXT_SCALE)?;

                println!(
                    "Generating params for {} sectors... It can take a couple of minutes ⌛",
                    file_name
                );
                let parameters = porep::generate_random_groth16_parameters(seal_proof)
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;
                parameters.write(&mut parameters_file)?;
                parameters.vk.write(&mut vk_file)?;

                let vk =
                    polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(parameters.vk)
                        .map_err(|e| UtilsCommandError::FromBytesError(e))?;
                let bytes = codec::Encode::encode(&vk);
                vk_scale_file.write_all(&bytes)?;

                println!("Generated parameters: ");
                println!("{}", parameters_file_name.display());
                println!("{}", vk_file_name.display());
                println!("{}", vk_scale_file_name.display());
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

                let sector_number = SectorNumber::try_from(sector_id)
                    .map_err(|_| UtilsCommandError::InvalidSectorId)?;

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
                let (proof_scale_filename, mut proof_scale_file) = file_with_extension(
                    &output_path,
                    format!("{}", sector_id).as_str(),
                    POREP_PROOF_EXT,
                )?;

                let mut source_file = tokio::fs::File::open(&input_path).await?;
                let mut car_v2_reader = CarV2Reader::new(&mut source_file);
                car_v2_reader
                    .is_car_file()
                    .await
                    .map_err(|e| UtilsCommandError::InvalidCARv2(input_path.clone(), e))?;

                let proof_parameters = porep::load_groth16_parameters(proof_parameters_path)
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                let piece_file = std::fs::File::open(&input_path)
                    .map_err(|e| UtilsCommandError::InvalidPieceFile(input_path.clone(), e))?;

                let piece_file_length = piece_file
                    .metadata()
                    .map_err(|e| UtilsCommandError::InvalidPieceFile(input_path, e))?
                    .len();

                let piece_file_length = PaddedPieceSize::from_arbitrary_size(piece_file_length);
                let piece_file = ZeroPaddingReader::new(piece_file, *piece_file_length.unpadded());

                let commp = cid::Cid::from_str(&commp)
                    .map_err(|e| UtilsCommandError::InvalidPieceCommP(commp, e))?;
                let piece_info = PieceInfo {
                    commitment: Commitment::try_from(commp)
                        .map_err(|e| UtilsCommandError::InvalidPieceType(commp.to_string(), e))?,
                    size: piece_file_length,
                };

                let (unsealed_sector_path, unsealed_sector) = file_with_extension(
                    &output_path,
                    format!("{}", sector_id).as_str(),
                    "sector.unsealed",
                )?;

                let (sealed_sector_path, _) = file_with_extension(
                    &output_path,
                    format!("{}", sector_id).as_str(),
                    "sector.sealed",
                )?;

                println!("Creating sector...");
                let sealer = Sealer::new(seal_proof);
                let piece_infos = sealer
                    .create_sector(vec![(piece_file, piece_info)], unsealed_sector)
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                let prover_id = derive_prover_id(signer.account_id());
                println!("Prover ID: {}", hex::encode(prover_id));

                println!("Precommitting...");
                let precommit = sealer
                    .precommit_sector(
                        &cache_directory,
                        unsealed_sector_path,
                        &sealed_sector_path,
                        prover_id,
                        sector_number,
                        ticket,
                        &piece_infos,
                    )
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                println!("Proving...");
                let proofs = sealer
                    .prove_sector(
                        &proof_parameters,
                        &cache_directory,
                        &sealed_sector_path,
                        prover_id,
                        sector_number,
                        ticket,
                        Some(seed),
                        precommit,
                        &piece_infos,
                    )
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                println!("CommD: {}", precommit.comm_d.cid());
                println!("CommR: {}", precommit.comm_r.cid());
                println!("Proof: {:?}", proofs);
                // We use sector size 2KiB only at this point, which guarantees to have 1 proof, because it has 1 partition in the config.
                // That's why `prove_commit` will always generate a 1 proof.
                let proof_scale: polka_storage_proofs::Proof<bls12_381::Bls12> = proofs[0]
                    .clone()
                    .try_into()
                    .expect("converstion between rust-fil-proofs and polka-storage-proofs to work");
                let scale_encoded_proof = codec::Encode::encode(&proof_scale);
                proof_scale_file.write_all(&scale_encoded_proof)?;

                println!("Proof as HEX: {}", hex::encode(scale_encoded_proof));
                println!("Wrote proof to {}", proof_scale_filename.display());
            }
            ProofsCommand::GeneratePoStParams {
                post_type,
                output_path,
            } => {
                let output_path = if let Some(output_path) = output_path {
                    output_path
                } else {
                    std::env::current_dir()?
                };

                let file_name: String = post_type.sector_size().to_string();

                let (parameters_file_name, mut parameters_file) =
                    file_with_extension(&output_path, file_name.as_str(), POST_PARAMS_EXT)?;
                let (vk_file_name, mut vk_file) =
                    file_with_extension(&output_path, file_name.as_str(), POST_VK_EXT)?;
                let (vk_scale_file_name, mut vk_scale_file) =
                    file_with_extension(&output_path, file_name.as_str(), POST_VK_EXT_SCALE)?;

                println!(
                    "Generating PoSt params for {} sectors... It can take a few secs ⌛",
                    file_name
                );
                let parameters = post::generate_random_groth16_parameters(post_type)
                    .map_err(|e| UtilsCommandError::GeneratePoStError(e))?;
                parameters.write(&mut parameters_file)?;
                parameters.vk.write(&mut vk_file)?;

                let vk =
                    polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(parameters.vk)
                        .map_err(|e| UtilsCommandError::FromBytesError(e))?;
                let bytes = codec::Encode::encode(&vk);
                vk_scale_file.write_all(&bytes)?;

                println!("Generated parameters: ");
                println!("{}", parameters_file_name.display());
                println!("{}", vk_file_name.display());
                println!("{}", vk_scale_file_name.display());
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

                let (proof_scale_filename, mut proof_scale_file) = file_with_extension(
                    &output_path,
                    format!("{}", sector_number).as_str(),
                    POST_PROOF_EXT,
                )?;

                let comm_r =
                    cid::Cid::from_str(&comm_r).map_err(|_| UtilsCommandError::CommRError)?;

                let sector_number = SectorNumber::try_from(sector_number)
                    .map_err(|_| UtilsCommandError::InvalidSectorId)?;

                let replicas = vec![ReplicaInfo {
                    sector_id: sector_number,
                    comm_r: comm_r
                        .hash()
                        .digest()
                        .try_into()
                        .map_err(|_| UtilsCommandError::CommRError)?,
                    replica_path,
                    cache_path: cache_directory,
                }];

                println!("Loading parameters...");
                let proof_parameters = post::load_groth16_parameters(proof_parameters_path)
                    .map_err(|e| UtilsCommandError::GeneratePoStError(e))?;

                let prover_id = derive_prover_id(signer.account_id());
                let proofs = post::generate_window_post(
                    post_type,
                    &proof_parameters,
                    randomness,
                    prover_id,
                    replicas,
                )
                .map_err(|e| UtilsCommandError::GeneratePoStError(e))?;

                println!("Proving...");
                // We only prove a single sector here, so it'll only be 1 proof.
                let proof_scale: polka_storage_proofs::Proof<bls12_381::Bls12> = proofs[0]
                    .clone()
                    .try_into()
                    .expect("converstion between rust-fil-proofs and polka-storage-proofs to work");
                proof_scale_file.write_all(&codec::Encode::encode(&proof_scale))?;
                println!("Wrote proof to {}", proof_scale_filename.display());
            }
        }

        Ok(())
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
}

fn file_with_extension(
    output_path: &PathBuf,
    file_name: &str,
    extension: &str,
) -> Result<(PathBuf, std::fs::File), UtilsCommandError> {
    let mut new_path = output_path.clone();
    new_path.push(file_name);
    new_path.set_extension(extension);

    let file = std::fs::File::create(new_path.clone())
        .map_err(|e| UtilsCommandError::FileCreateError(new_path.clone(), e))?;
    Ok((new_path, file))
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
