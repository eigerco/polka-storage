use std::path::PathBuf;

use clap::Parser;
use mater::Error;

use crate::{convert::convert_file_to_car, extract::extract_file_from_car};

mod convert;
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

        /// If enabled, the output will overwrite any existing files.
        #[arg(long, action)]
        overwrite: bool,

        /// If enabled, the file will be wrapped with UnixFS information.
        #[arg(long, action)]
        wrap: bool,
    },
    /// Convert a CARv2 file to its original format
    Extract {
        /// Path to CARv2 file
        input_path: PathBuf,
        /// Path to output file
        output_path: Option<PathBuf>,
        /// If enabled, the output will overwrite any existing files.
        #[arg(long, action)]
        overwrite: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    match MaterCli::parse() {
        MaterCli::Convert {
            input_path,
            output_path,
            quiet,
            overwrite,
            wrap,
        } => {
            let output_path = output_path.unwrap_or_else(|| {
                // If we let the output become `-.car` it isn't only weird
                // terminals are annoying with it
                if input_path.as_os_str() == "-" {
                    let file_name = "stdin.car";
                    let mut path = PathBuf::with_capacity(file_name.len());
                    path.set_file_name(file_name);
                    path
                } else {
                    let mut new_path = input_path.clone();
                    new_path.set_extension("car");
                    new_path
                }
            });
            let cid = convert_file_to_car(&input_path, &output_path, overwrite, wrap).await?;

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
            overwrite,
        } => {
            let output_path = output_path.unwrap_or_else(|| {
                // If we let the output become `-` it isn't only weird
                // terminals are annoying with it
                if input_path.as_os_str() == "-" {
                    let file_name = "stdin";
                    let mut path = PathBuf::with_capacity(file_name.len());
                    path.set_file_name(file_name);
                    path
                } else {
                    let mut new_path = input_path.clone();
                    if let Some(_) = new_path.extension() {
                        // We don't check if the ext is `.car` because we don't care about the ext
                        // being "right", we only care about the bytes, however, we remove the ext
                        // if there is one
                        new_path.set_extension("");
                    } else {
                        // If no extension existed, we must add one or we risk overwriting the file
                        new_path.set_extension("out");
                    }
                    new_path
                }
            });
            extract_file_from_car(&input_path, &output_path, overwrite).await?;

            println!(
                "Successfully converted CARv2 file {} and saved it to to {}",
                input_path.display(),
                output_path.display()
            );
        }
    }

    Ok(())
}
