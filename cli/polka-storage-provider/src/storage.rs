use std::path::{Path, PathBuf};

use mater::{create_filestore, Cid, Config};
use tempfile::tempdir;
use tokio::{
    fs::{self, File},
    io::{AsyncRead, BufWriter},
};
use tracing::info;

/// Directory where uploaded files are stored.
pub const STORAGE_DEFAULT_DIRECTORY: &str = "./uploads";

/// Reads bytes from the source and writes them to a CAR file.
pub async fn stream_contents_to_car<R>(
    folder: &str,
    source: R,
) -> Result<Cid, Box<dyn std::error::Error>>
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
    let (_, final_content_path) = content_path(folder, cid);
    fs::rename(temp_file_path, &final_content_path).await?;
    info!(location = %final_content_path.display(), "CAR file created");

    Ok(cid)
}

/// Returns the tuple of file name and path for a specified Cid.
pub fn content_path(folder: &str, cid: Cid) -> (String, PathBuf) {
    let name = format!("{cid}.car");
    let path = Path::new(folder).join(&name);
    (name, path)
}
