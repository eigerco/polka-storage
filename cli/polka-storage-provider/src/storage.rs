use std::path::{Path, PathBuf};

use mater::{create_filestore, Cid, Config};
use tempfile::tempdir;
use tokio::{
    fs::{self, File},
    io::{AsyncRead, BufWriter},
};

/// Directory where uploaded files are stored.
const UPLOADS_DIRECTORY: &str = "uploads";

/// Reads bytes from the source and writes them to a CAR file.
pub async fn stream_contents_to_car<R>(source: R) -> Result<Cid, Box<dyn std::error::Error>>
where
    R: AsyncRead + Unpin,
{
    // Temp file which will be used to store the CAR file content. The temp
    // director has a randomized name.
    let temp_dir = tempdir()?;
    let temp_file_path = temp_dir.path().join("temp.car");

    // Stream the body from source to the temp file.
    let file = File::create(&temp_file_path).await?;
    let writer = BufWriter::new(file);
    let cid = create_filestore(source, writer, Config::default()).await?;

    // If the file is successfully written, we can now move it to the final
    // location.
    let final_content_path = content_path(cid);
    fs::rename(temp_file_path, final_content_path).await?;

    Ok(cid)
}

/// Returns the path to the content with the specified CID.
pub fn content_path(cid: Cid) -> PathBuf {
    let name = format!("{cid}.car");
    Path::new(UPLOADS_DIRECTORY).join(name)
}
