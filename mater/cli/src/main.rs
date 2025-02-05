use std::path::PathBuf;

use clap::Parser;
use mater::{Config, DEFAULT_CHUNK_SIZE, DEFAULT_TREE_WIDTH};

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

        /// If enabled, only the resulting CID will be printed.
        #[arg(short, long, action)]
        quiet: bool,

        /// If enabled, content will be stored directly without UnixFS wrapping.
        #[arg(long, action)]
        raw: bool,

        /// Size of each chunk in bytes.
        #[arg(long, default_value_t = DEFAULT_CHUNK_SIZE)]
        chunk_size: usize,

        /// Maximum number of children per parent node.
        #[arg(long, default_value_t = DEFAULT_TREE_WIDTH)]
        tree_width: usize,
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
            quiet,
            raw,
            chunk_size,
            tree_width,
        } => {
            let output_path = output_path.unwrap_or_else(|| {
                let mut new_path = input_path.clone();
                new_path.set_extension("car");
                new_path
            });

            // Build config with UnixFS wrapping by default
            let config = Config::balanced(
                chunk_size,
                tree_width,
                raw,
            );

            let cid = convert_file_to_car(&input_path, &output_path, config, false).await?;

            if quiet {
                println!("{}", cid);
            } else {
                println!(
                    "Converted {} and saved the CARv2 file at {} with a CID of {cid}",
                    input_path.display(),
                    output_path.display()
                );
            }
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
                "Successfully converted CARv2 file {} and saved it to {}",
                input_path.display(),
                output_path.display()
            );
        }
    }
    Ok(())
}
