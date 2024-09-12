use std::path::PathBuf;

use mater::{create_filestore, Config};
use tokio::fs::File;

use crate::error::Error;

/// Converts a file at location `input_path` to a CARv2 file at `output_path`
pub(crate) async fn convert_file_to_car(
    input_path: PathBuf,
    output_path: PathBuf,
) -> Result<(), Error> {
    let source_file = File::open(&input_path).await?;
    let output_file = File::create(&output_path).await?;
    let _cid = create_filestore(source_file, output_file, Config::default()).await?;
    println!(
        "Converted {} and saved the CARv2 file at {}",
        input_path.display(),
        output_path.display()
    );
    Ok(())
}

/// Tests for file conversion.
/// MaterError cases are not handled because these are tested in the mater library.
#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;
    use tokio::fs::File;

    use crate::{convert::convert_file_to_car, error::Error};

    #[tokio::test]
    async fn convert_file_to_car_success() -> Result<()> {
        // Setup: Create a dummy input file
        let temp_dir = tempdir()?;
        let input_path = temp_dir.path().join("test_input.txt");
        let mut input_file = File::create(&input_path).await?;
        tokio::io::AsyncWriteExt::write_all(&mut input_file, b"test data").await?;

        // Define output path
        let output_path = temp_dir.path().join("test_output.car");

        // Call the function under test
        let result = convert_file_to_car(input_path.clone(), output_path.clone()).await;

        // Assert the result is Ok
        assert!(result.is_ok());

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

        // Call the function under test
        let result = convert_file_to_car(input_path.clone(), output_path.clone()).await;

        // Assert the result is an error
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::IoError(..))));

        // Close temporary directory
        temp_dir.close()?;

        Ok(())
    }
}
