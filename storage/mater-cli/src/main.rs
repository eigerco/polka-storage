use std::{fs::File, path::PathBuf};

use clap::Parser;

use crate::{
    commp::{calculate_piece_commitment, piece_commitment_cid},
    convert::convert_file_to_car,
    error::Error,
    extract::extract_file_from_car,
};

mod commp;
mod convert;
mod error;
mod extract;

/// Command-line interface for converting files to and from CAR format.
/// Supports converting a file to CAR format and extracting a CAR file to its original format.
/// Uses async functions to handle file operations efficiently.
#[derive(Parser)]
enum MaterCli {
    /// Convert a file to CARv2 format
    Convert {
        /// Path to input file
        input_path: PathBuf,
        /// Optional path to output CARv2 file.
        /// If no output path is given it will store the `.car` file in the same location.
        output_path: Option<PathBuf>,
    },
    /// Convert a CARv2 file to its original format
    Extract {
        /// Path to CARv2 file
        input_path: PathBuf,
        /// Path to output file
        output_path: Option<PathBuf>,
    },
    /// Calculate a piece commitment for the provided data stored at the a given path
    #[clap(alias = "commp")]
    CalculatePieceCommitment {
        /// Path to the data
        input_path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    match MaterCli::parse() {
        MaterCli::Convert {
            input_path,
            output_path,
        } => {
            let output_path = output_path.unwrap_or_else(|| {
                let mut new_path = input_path.clone();
                new_path.set_extension("car");
                new_path
            });
            let cid = convert_file_to_car(&input_path, &output_path).await?;

            println!(
                "Converted {} and saved the CARv2 file at {} with a CID of {cid}",
                input_path.display(),
                output_path.display()
            );
        }
        MaterCli::Extract {
            input_path,
            output_path,
        } => {
            let output_path = output_path.unwrap_or_else(|| {
                let mut new_path = input_path.clone();
                new_path.set_extension("");
                new_path
            });
            extract_file_from_car(&input_path, &output_path).await?;

            println!(
                "Successfully converted CARv2 file {} and saved it to to {}",
                input_path.display(),
                output_path.display()
            );
        }
        MaterCli::CalculatePieceCommitment { input_path } => {
            let mut source_file = File::open(&input_path)?;
            let file_size = source_file.metadata()?.len();

            let commitment = calculate_piece_commitment(&mut source_file, file_size)?;
            let cid = piece_commitment_cid(commitment);

            println!("Piece commitment CID: {cid}");
        }
    }

    Ok(())
}
