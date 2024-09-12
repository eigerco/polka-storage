use std::path::PathBuf;

use mater::CarV2Reader;
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt, BufReader},
};

use crate::error::Error;

/// Extracts a file to `output_path` from the CARv2 file at `input_path`
pub(crate) async fn extract_file_from_car(
    input_path: PathBuf,
    output_path: PathBuf,
) -> Result<(), Error> {
    let source_file = File::open(&input_path).await?;
    let mut output_file = File::create(&output_path).await?;
    let size = source_file.metadata().await?.len();

    // Avoid doing any work if the file is empty.
    // Still create CAR file and inform the user.
    if size == 0 {
        println!("Supplied CAR file is empty");
        print_successful_extraction(input_path, output_path);
        return Ok(());
    }

    let mut reader = CarV2Reader::new(BufReader::new(source_file));
    reader.read_pragma().await?;
    let header = reader.read_header().await?;
    let _v1_header = reader.read_v1_header().await?;
    let mut written = 0;

    while let Ok((_cid, contents)) = reader.read_block().await {
        let position = reader.get_inner_mut().stream_position().await?;
        let data_end = header.data_offset + header.data_size;
        // Add the `written != 0` clause for files that are less than a single block.
        if position >= data_end && written != 0 {
            break;
        }
        written += output_file.write(&contents).await?;
    }
    output_file.flush().await?;
    print_successful_extraction(input_path, output_path);
    Ok(())
}

fn print_successful_extraction(input_path: PathBuf, output_path: PathBuf) {
    println!(
        "Successfully converted CARv2 file {} and saved it to to {}",
        input_path.display(),
        output_path.display()
    );
}

/// Tests for file extraction.
/// MaterError cases are not handled because these are tested in the mater library
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use tempfile::tempdir;
    use tokio::{fs::File, io::AsyncReadExt};

    use crate::{error::Error, extract_file_from_car};

    /// Tests successful extraction of contents from a CARv2 file
    #[tokio::test]
    async fn extract_file_success() -> Result<()> {
        // Setup input and output paths
        let temp_dir = tempdir()?;
        let input_path = PathBuf::from("../mater/tests/fixtures/car_v2/lorem.car");
        let output_path = temp_dir.path().join("output_file");

        // Call the function under test
        let result = extract_file_from_car(input_path, output_path.clone()).await;
        // Assert the function succeeded
        assert!(result.is_ok());

        // extract original contents
        let mut original = File::open("../mater/tests/fixtures/original/lorem.txt").await?;
        let mut original_contents = vec![];
        original.read_to_end(&mut original_contents).await?;

        // extract output file
        let mut output_file = File::open(output_path).await?;
        let mut output_contents = vec![];
        output_file.read_to_end(&mut output_contents).await?;

        // Verify the output file is created and contains expected data
        assert_eq!(output_contents, original_contents);

        // Close temporary directory
        temp_dir.close()?;

        Ok(())
    }

    /// Tests IO error for a file that does not exist
    #[tokio::test]
    async fn io_error_extract_non_existent_file() -> Result<()> {
        // Setup input and output paths
        let temp_dir = tempdir()?;
        let input_path = temp_dir.path().join("test_data/non_existent.car");
        let output_path = temp_dir.path().join("test_output/output_file");

        // Call the function under test
        let result = extract_file_from_car(input_path.clone(), output_path.clone()).await;

        // Assert the function returns an error
        assert!(result.is_err());
        // Verify the error is of type Error
        assert!(matches!(result, Err(Error::IoError(..))));

        Ok(())
    }
}
