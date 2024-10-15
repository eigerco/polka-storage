use std::{
    fs::File,
    io::{BufReader, Write},
    path::PathBuf,
    str::FromStr,
};

use mater::CarV2Reader;
use polka_storage_proofs::{
    porep::{self, sealer::Sealer},
    post,
    types::PieceInfo,
};
use polka_storage_provider_common::commp::{
    calculate_piece_commitment, CommPError, ZeroPaddingReader,
};
use primitives_commitment::piece::PaddedPieceSize;
use primitives_proofs::{RegisteredPoStProof, RegisteredSealProof};

use crate::CliError;

/// Utils sub-commands.
#[derive(Debug, clap::Subcommand)]
pub enum UtilsCommand {
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
        seal_proof: PoRepSealProof,
        /// Directory where the params files will be put. Defaults to the current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
    },
    /// **DEMO COMMAND** IT SHOULD NOT BE USED IN PRODUCTION AND ITS FLOW IS SKEWED!
    /// Generates PoRep for a piece file.
    /// Takes a piece file (in a CARv2 archive, unpadded), puts it into a sector (temp file), seals and proves it.
    PoRep {
        /// PoRep has multiple variants dependent on the sector size.
        /// Parameters are required for each sector size and its corresponding PoRep Params.
        #[arg(short, long, default_value = "2KiB")]
        seal_proof: PoRepSealProof,
        /// Path to where parameters to corresponding `seal_proof` are stored.
        #[arg(short, long)]
        proof_parameters_path: PathBuf,
        /// Piece file, CARv2 archive created with `mater-cli convert`.
        input_path: PathBuf,
        /// CommP of a file, calculated with `commp` command.
        commp: String,
        /// Directory where the proof files will be put. Defaults to the current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
    },
    /// Generates PoSt verifying key and proving parameters for zk-SNARK workflows (submit windowed PoSt)
    #[clap(name = "post-params")]
    GeneratePoStParams {
        /// PoSt has multiple variants dependant on the sector size.
        /// Parameters are required for each sector size and its corresponding PoSt.
        #[arg(short, long, default_value = "2KiB")]
        post_type: PoStProof,
        /// Directory where the params files will be put. Defaults to current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
    },
}

const POREP_PARAMS_EXT: &str = ".porep.params";
const POREP_VK_EXT: &str = ".porep.vk";
const POREP_VK_EXT_SCALE: &str = ".porep.vk.scale";

const POST_PARAMS_EXT: &str = ".post.params";
const POST_VK_EXT: &str = ".post.vk";
const POST_VK_EXT_SCALE: &str = ".post.vk.scale";

impl UtilsCommand {
    /// Run the command.
    pub async fn run(self) -> Result<(), CliError> {
        match self {
            UtilsCommand::CalculatePieceCommitment { input_path } => {
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
            UtilsCommand::GeneratePoRepParams {
                seal_proof,
                output_path,
            } => {
                let output_path = if let Some(output_path) = output_path {
                    output_path
                } else {
                    std::env::current_dir()?
                };

                let file_name: String = seal_proof.clone().into();

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
                let parameters = porep::generate_random_groth16_parameters(seal_proof.0)
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
            UtilsCommand::PoRep {
                seal_proof,
                proof_parameters_path,
                input_path,
                commp,
                output_path,
            } => {
                let output_path = if let Some(output_path) = output_path {
                    output_path
                } else {
                    std::env::current_dir()?
                };
                let (proof_scale_filename, mut proof_scale_file) = file_with_extension(
                    &output_path,
                    input_path
                        .file_name()
                        .expect("input file to have a name")
                        .to_str()
                        .expect("to be convertable to str"),
                    "proof.scale",
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
                let commp = cid::Cid::from_str(commp.as_str())
                    .map_err(|e| UtilsCommandError::InvalidPieceCommP(commp, e))?;
                let piece_info = PieceInfo {
                    commitment: commp
                        .hash()
                        .digest()
                        .try_into()
                        .expect("CommPs guaranteed to be 32 bytes"),
                    size: piece_file_length,
                };

                let mut unsealed_sector =
                    tempfile::NamedTempFile::new().map_err(|e| UtilsCommandError::IOError(e))?;
                let sealed_sector =
                    tempfile::NamedTempFile::new().map_err(|e| UtilsCommandError::IOError(e))?;

                println!("Creating sector...");
                let sealer = Sealer::new(seal_proof.0);
                let piece_infos = sealer
                    .create_sector(
                        vec![(piece_file, piece_info.clone())],
                        unsealed_sector.as_file_mut(),
                    )
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                // Those are hardcoded for the showcase only.
                // They should come from Storage Provider Node, precommits and other information.
                let sector_id = 77;
                let prover_id = [0u8; 32];
                let ticket = [12u8; 32];
                let seed = [13u8; 32];

                println!("Precommitting...");
                let cache_directory =
                    tempfile::tempdir().map_err(|e| UtilsCommandError::IOError(e))?;
                let precommit = sealer
                    .precommit_sector(
                        cache_directory.path(),
                        unsealed_sector.path(),
                        sealed_sector.path(),
                        prover_id,
                        sector_id,
                        ticket,
                        &piece_infos,
                    )
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                println!("Proving...");
                let proofs = sealer
                    .prove_sector(
                        &proof_parameters,
                        cache_directory.path(),
                        sealed_sector.path(),
                        prover_id,
                        sector_id,
                        ticket,
                        Some(seed),
                        precommit.clone(),
                        &piece_infos,
                    )
                    .map_err(|e| UtilsCommandError::GeneratePoRepError(e))?;

                println!("CommR: {:?}", hex::encode(&precommit.comm_r));
                println!("CommD: {:?}", hex::encode(&precommit.comm_d));
                println!("Proof: {:?}", proofs);
                // We use sector size 2KiB only at this point, which guarantees to have 1 proof, because it has 1 partition in the config.
                // That's why `prove_commit` will always generate a 1 proof.
                let proof_scale: polka_storage_proofs::Proof<bls12_381::Bls12> = proofs[0]
                    .clone()
                    .try_into()
                    .expect("converstion between rust-fil-proofs and polka-storage-proofs to work");
                proof_scale_file.write_all(&codec::Encode::encode(&proof_scale))?;

                println!("Wrote proof to {}", proof_scale_filename.display());
            }
            UtilsCommand::GeneratePoStParams {
                post_type,
                output_path,
            } => {
                let output_path = if let Some(output_path) = output_path {
                    output_path
                } else {
                    std::env::current_dir()?
                };

                let file_name: String = post_type.clone().into();

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
                let parameters = post::generate_random_groth16_parameters(post_type.0)
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
    #[error("failed to load piece file at path: {0}")]
    InvalidPieceFile(PathBuf, std::io::Error),
    #[error("provided invalid CommP {0}, error: {1}")]
    InvalidPieceCommP(String, cid::Error),
    #[error(transparent)]
    IOError(std::io::Error),
    #[error("file {0} is invalid CARv2 file {1}")]
    InvalidCARv2(PathBuf, mater::Error),
}

#[derive(Clone, Debug)]
pub struct PoRepSealProof(RegisteredSealProof);

impl std::str::FromStr for PoRepSealProof {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "2KiB" => Ok(PoRepSealProof(RegisteredSealProof::StackedDRG2KiBV1P1)),
            v => Err(format!("unknown value for RegisteredSealProof: {}", v)),
        }
    }
}

impl Into<String> for PoRepSealProof {
    fn into(self) -> String {
        match self.0 {
            RegisteredSealProof::StackedDRG2KiBV1P1 => "2KiB".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PoStProof(RegisteredPoStProof);

impl std::str::FromStr for PoStProof {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "2KiB" => Ok(PoStProof(RegisteredPoStProof::StackedDRGWindow2KiBV1P1)),
            v => Err(format!("unknown value for RegisteredPoStProof: {}", v)),
        }
    }
}

impl Into<String> for PoStProof {
    fn into(self) -> String {
        match self.0 {
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => "2KiB".into(),
        }
    }
}

fn file_with_extension(
    output_path: &PathBuf,
    file_name: &str,
    extension: &str,
) -> Result<(PathBuf, std::fs::File), UtilsCommandError> {
    let mut new_path = output_path.clone();
    new_path.push(file_name);
    new_path.set_extension(extension);

    let file = std::fs::File::create_new(new_path.clone())
        .map_err(|e| UtilsCommandError::FileCreateError(new_path.clone(), e))?;
    Ok((new_path, file))
}
