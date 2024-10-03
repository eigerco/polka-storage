mod commp;

use std::{fs::File, io::Write, path::PathBuf};

use mater::CarV2Reader;
use polka_storage_proofs::porep;
use primitives_proofs::RegisteredSealProof;
use primitives_shared::{
    piece::PaddedPieceSize
};
use std::io::BufReader;

use crate::{
    commands::utils::commp::{calculate_piece_commitment, CommPError, ZeroPaddingReader},
    CliError,
};

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
        /// PoRep has multiple variants dependant on the sector size.
        /// Parameters are required for each sector size and its corresponding PoRep.
        #[arg(short, long, default_value = "2KiB")]
        seal_proof: PoRepSealProof,
        /// Directory where the params files will be put. Defaults to current directory.
        #[arg(short, long)]
        output_path: Option<PathBuf>,
    },
}

impl UtilsCommand {
    /// Run the command.
    pub async fn run(self) -> Result<(), CliError> {
        match self {
            UtilsCommand::CalculatePieceCommitment { input_path } => {
                // Check if the file is a CARv2 file. If it is, we can't calculate the piece commitment.
                let mut source_file = tokio::fs::File::open(&input_path).await?;
                let mut car_v2_reader = CarV2Reader::new(&mut source_file);
                car_v2_reader.is_car_file().await?;

                // Calculate the piece commitment.
                let source_file = File::open(&input_path)?;
                let file_size = source_file.metadata()?.len();

                let buffered = BufReader::new(source_file);
                let padded_piece_size = PaddedPieceSize::new(file_size.next_power_of_two() as u64)
                    .expect("is power of two");
                let mut zero_padding_reader =
                    ZeroPaddingReader::new(buffered, *padded_piece_size as usize);

                // The calculate_piece_commitment blocks the thread. We could
                // use tokio::task::spawn_blocking to avoid this, but in this
                // case it doesn't matter because this is the only thing we are
                // working on.
                let commitment =
                    calculate_piece_commitment(&mut zero_padding_reader, padded_piece_size)
                        .map_err(|err| UtilsCommandError::CommPError(err))?;
                let cid = commitment.cid();

                println!("Piece commitment CID: {cid}");
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

                let mut parameters_file =
                    file_with_extension(&output_path, file_name.as_str(), "params")?;
                let mut vk_file = file_with_extension(&output_path, file_name.as_str(), "vk")?;
                let mut vk_scale_file =
                    file_with_extension(&output_path, file_name.as_str(), "vk.scale")?;

                println!(
                    "Generating params for {} sectors... It can take a couple of minutes ⌛",
                    file_name
                );
                let parameters =
                    porep::generate_random_groth16_parameters(seal_proof.0).expect("work pls");
                parameters.write(&mut parameters_file)?;
                parameters.vk.write(&mut vk_file)?;

                let vk =
                    polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(parameters.vk)
                        .map_err(|e| UtilsCommandError::FromBytesError(e))?;
                let bytes = codec::Encode::encode(&vk);
                vk_scale_file.write_all(&bytes)?;

                println!(
                    "Wrote {1}.params, {1}.vk, {1}.vk.scale into {} directory",
                    output_path.display(),
                    file_name
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UtilsCommandError {
    #[error("the commp command failed because: {0}")]
    CommPError(#[from] CommPError),
    #[error("CidError: {0}")]
    CidError(String),
    #[error("failed to create a file '{0}' because: {1}")]
    FileCreateError(PathBuf, std::io::Error),
    #[error("failed to convert from rust-fil-proofs to polka-storage-proofs: {0}")]
    FromBytesError(#[from] polka_storage_proofs::FromBytesError),
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

fn file_with_extension(
    output_path: &PathBuf,
    file_name: &str,
    extension: &str,
) -> Result<std::fs::File, UtilsCommandError> {
    let mut new_path = output_path.clone();
    new_path.push(file_name);
    new_path.set_extension(extension);
    std::fs::File::create_new(new_path.clone())
        .map_err(|e| UtilsCommandError::FileCreateError(new_path, e))
}
