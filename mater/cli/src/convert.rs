use std::path::PathBuf;

use mater::{create_filestore, Cid, Config};
use tokio::fs::File;

use crate::error::Error;

/// Converts a file at location `input_path` to a CARv2 file at `output_path`
pub(crate) async fn convert_file_to_car(
    input_path: &PathBuf,
    output_path: &PathBuf,
    config: Config,
) -> Result<Cid, Error> {
    let source_file = File::open(input_path).await?;
    let output_file = File::create(output_path).await?;
    let cid = create_filestore(source_file, output_file, config).await?;
    Ok(cid)
}

/// Tests for file conversion.
/// MaterError cases are not handled because these are tested in the mater library.
#[cfg(test)]
mod tests {
    use anyhow::Result;
    use mater::Cid;
    use mater::Config;
    use std::str::FromStr;
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
        let config = Config::balanced_raw(256 * 1024, 174);

        // Call the function under test
        let result = super::convert_file_to_car(&input_path, &output_path, config).await;
        assert!(result.is_ok());
        assert_eq!(result?, expected_cid);

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
        let result = super::convert_file_to_car(&input_path, &output_path, config).await;
        assert!(result.is_err());
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

        // Create output file
        let output_path = temp_dir.path().join("output_file");
        File::create_new(&output_path).await?;

        let config = Config::default();

        // Call the function under test
        let result = super::convert_file_to_car(&input_path, &output_path, config).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(super::Error::IoError(..))));

        Ok(())
    }
}
