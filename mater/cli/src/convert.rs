use std::path::PathBuf;

use mater::{create_filestore, Cid, Config};
use tokio::fs::File;

use crate::error::Error;

/// Converts a file at location `input_path` to a CARv2 file at `output_path`
pub(crate) async fn convert_file_to_car(
    input_path: &PathBuf,
    output_path: &PathBuf,
    config: Config,
    overwrite: bool,
) -> Result<Cid, Error> {
    let source_file = File::open(input_path).await?;
    let output_file = if overwrite {
        File::create(output_path).await
    } else {
        File::create_new(output_path).await
    }?;
    let cid = create_filestore(source_file, output_file, config).await?;

    Ok(cid)
}

/// Tests for file conversion.
/// MaterError cases are not handled because these are tested in the mater library.
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use anyhow::Result;
    use mater::{Cid, Config, DEFAULT_CHUNK_SIZE, DEFAULT_TREE_WIDTH};
    use tempfile::tempdir;
    use tokio::{fs::File, io::AsyncWriteExt};

    #[tokio::test]
    async fn convert_file_to_car_raw_success() -> Result<()> {
        // Setup: Create a dummy input file
        let temp_dir = tempdir()?;
        let input_path = temp_dir.path().join("test_input.txt");
        let mut input_file = File::create(&input_path).await?;
        input_file.write_all(b"test data").await?;

        let expected_cid =
            Cid::from_str("bafkreiern4acpjlva5gookrtc534gr4nmuj7pbvfsg6yslnbuv336izv7e")?;

        // Define output path
        let output_path = temp_dir.path().join("test_output.car");

        // Configure in raw mode
        let config = Config::balanced_raw(DEFAULT_CHUNK_SIZE, DEFAULT_TREE_WIDTH);

        // Call the function under test
        let result = super::convert_file_to_car(&input_path, &output_path, config, false).await;

        // Assert the result is Ok
        assert!(result.is_ok());

        // Verify that the CID is as expected
        assert_eq!(result?, expected_cid);

        // Close temporary directory
        temp_dir.close()?;

        Ok(())
    }

    #[tokio::test]
    async fn io_error_convert_non_existent_file() -> Result<()> {
        // Define non-existent input path
        let temp_dir = tempdir()?;
        let input_path = temp_dir.path().join("non_existent_input.txt");

        // Define output path
        let output_path = temp_dir.path().join("test_output.car");

        let config = Config::default();

        // Call the function under test
        let result = super::convert_file_to_car(&input_path, &output_path, config, false).await;
        assert!(matches!(result, Err(super::Error::IoError(..))));

        Ok(())
    }

    #[tokio::test]
    async fn io_error_convert_file_exists() -> Result<()> {
        // Setup: Create a dummy input file
        let temp_dir = tempdir()?;
        let input_path = temp_dir.path().join("test_input.txt");
        let mut input_file = File::create(&input_path).await?;
        tokio::io::AsyncWriteExt::write_all(&mut input_file, b"test data").await?;

        // Create output file so that the file already exists.
        // Since we are not allowing overwrites (overwrite = false), this should trigger an error.
        let output_path = temp_dir.path().join("output_file");
        File::create_new(&output_path).await?;
        println!("gets here");

        // Provide a configuration (using default in this example).
        let config = Config::default();

        // Call the function under test with the config and overwrite flag set to false.
        let result = super::convert_file_to_car(&input_path, &output_path, config, false).await;

        // Assert the result is an error, specifically an IoError.
        assert!(result.is_err());
        assert!(matches!(result, Err(super::Error::IoError(..))));

        temp_dir.close()?;
        Ok(())
    }
}
