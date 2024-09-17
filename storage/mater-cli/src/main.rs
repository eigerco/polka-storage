use std::path::PathBuf;

use clap::Parser;

use crate::{convert::convert_file_to_car, error::Error, extract::extract_file_from_car};

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
            convert_file_to_car(input_path, output_path).await?
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
            extract_file_from_car(input_path, output_path).await?
        }
    }

    Ok(())
}
