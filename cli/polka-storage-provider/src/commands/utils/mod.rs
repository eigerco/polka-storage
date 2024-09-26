mod commp;

use std::{fs::File, path::PathBuf};

use mater::CarV2Reader;

use crate::{
    commands::utils::commp::{calculate_piece_commitment, piece_commitment_cid, CommPError},
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
                let mut source_file = File::open(&input_path)?;
                let file_size = source_file.metadata()?.len();

                // The calculate_piece_commitment blocks the thread. We could
                // use tokio::task::spawn_blocking to avoid this, but in this
                // case it doesn't matter because this is the only thing we are
                // working on.
                let commitment = calculate_piece_commitment(&mut source_file, file_size)
                    .map_err(|err| UtilsCommandError::CommPError(err))?;
                let cid = piece_commitment_cid(commitment);

                println!("Piece commitment CID: {cid}");
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UtilsCommandError {
    #[error("the commp command failed with the following error: {0}")]
    CommPError(#[from] CommPError),
}
