use std::io::{BufReader, Read};

use filecoin_hashers::{
    sha256::{Sha256Domain, Sha256Hasher},
    Domain,
};
use fr32::{to_padded_bytes, Fr32Reader};
use ipld_core::cid::multihash::Multihash;
use mater::Cid;
use storage_proofs_core::{merkle::BinaryMerkleTree, util::NODE_SIZE};
use thiserror::Error;

/// SHA2-256 with the two most significant bits from the last byte zeroed (as
/// via a mask with 0b00111111) - used for proving trees as in Filecoin.
///
/// https://github.com/multiformats/multicodec/blob/badcfe56bb7e0bbb06b60d57565186cd6be1f932/table.csv#L153
pub const SHA2_256_TRUNC254_PADDED: u64 = 0x1012;

/// Filecoin piece or sector data commitment merkle node/root (CommP & CommD)
///
/// https://github.com/multiformats/multicodec/blob/badcfe56bb7e0bbb06b60d57565186cd6be1f932/table.csv#L554
pub const FIL_COMMITMENT_UNSEALED: u64 = 0xf101;

/// Reader that returns zeros if the inner reader is empty.
struct ZeroPaddingReader<R: Read> {
    inner: R,
    remaining: usize,
}

impl<R: Read> ZeroPaddingReader<R> {
    fn new(inner: R, total_size: usize) -> Self {
        Self {
            inner,
            remaining: total_size,
        }
    }
}

impl<R: Read> Read for ZeroPaddingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }

        let to_read = buf.len().min(self.remaining);
        let read = self.inner.read(&mut buf[..to_read])?;

        if read < to_read {
            buf[read..to_read].fill(0);
        }

        self.remaining -= to_read;
        Ok(to_read)
    }
}

/// Ensure that the padded piece size is valid before
fn ensure_piece_size(padded_piece_size: usize) -> Result<(), CommPError> {
    if padded_piece_size < NODE_SIZE {
        return Err(CommPError::PieceTooSmall);
    }

    if padded_piece_size % NODE_SIZE != 0 {
        return Err(CommPError::InvalidPieceSize(format!(
            "padded_piece_size is not multiple of {NODE_SIZE}"
        )));
    }

    Ok(())
}

/// Calculate the piece commitment for a given data source.
///
/// https://spec.filecoin.io/systems/filecoin_files/piece/#section-systems.filecoin_files.piece.data-representation
pub fn calculate_piece_commitment<R: Read>(
    source: R,
    unpadded_piece_size: u64,
) -> Result<[u8; 32], CommPError> {
    // Wrap the source in a BufReader for efficient reading.
    let source = BufReader::new(source);
    // This reader adds two zero bits to each 254 bits of data read from the source.
    let fr32_reader = Fr32Reader::new(source);
    // This is the padded piece size after we add 2 zero bits to each 254 bits of data.
    let padded_piece_size = to_padded_bytes(unpadded_piece_size as usize);
    // Final padded piece size should be 2^n where n is a positive integer. That
    // is because we are using MerkleTree to calculate the piece commitment.
    let padded_piece_size = padded_piece_size.next_power_of_two();

    // Ensure that the piece size is valid, before generating a MerkeTree.
    ensure_piece_size(padded_piece_size)?;

    // The reader that pads the source with zeros
    let mut zero_padding_reader = ZeroPaddingReader::new(fr32_reader, padded_piece_size);

    // Buffer used for reading data used for leafs.
    let mut buffer = [0; NODE_SIZE];
    // Number of leafs
    let num_leafs = (padded_piece_size as f64 / NODE_SIZE as f64).ceil() as usize;

    // Elements iterator used by the MerkleTree. The elements returned by the
    // iterator represent leafs of the tree
    let elements_iterator = (0..num_leafs).map(|_| {
        zero_padding_reader.read_exact(&mut buffer)?;
        let hash = Sha256Domain::try_from_bytes(&buffer)?;
        Ok(hash)
    });
    let tree = BinaryMerkleTree::<Sha256Hasher>::try_from_iter(elements_iterator)
        .map_err(|err| CommPError::TreeBuildError(err.to_string()))?;

    // Read and return the root of the tree
    let mut commitment = [0; NODE_SIZE];
    tree.root()
        .write_bytes(&mut commitment)
        .expect("destination large enough"); // This is safe because our `comm_p_bytes` is 32 bytes long

    Ok(commitment)
}

/// Generate Cid from the piece commitment
pub fn piece_commitment_cid(piece_commitment: [u8; 32]) -> Cid {
    let hash = Multihash::wrap(SHA2_256_TRUNC254_PADDED, &piece_commitment)
        .expect("piece commitment not more than 64 bytes");
    Cid::new_v1(FIL_COMMITMENT_UNSEALED, hash)
}

#[derive(Debug, Error)]
pub enum CommPError {
    #[error("Piece is too small")]
    PieceTooSmall,
    #[error("Piece is not valid size: {0}")]
    InvalidPieceSize(String),
    #[error("Tree build error: {0}")]
    TreeBuildError(String),
    #[error("IOError: {0}")]
    IOError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use crate::commp::{calculate_piece_commitment, CommPError, ZeroPaddingReader};

    #[test]
    fn test_zero_padding_reader() {
        let data = vec![1, 2, 3, 4, 5, 6];
        let total_size = 10;
        let mut reader = ZeroPaddingReader::new(&data[..], total_size);

        let mut buffer = [0; 4];
        // First read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 4);
        assert_eq!(buffer, [1, 2, 3, 4]);
        // Second read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 4);
        assert_eq!(buffer, [5, 6, 0, 0]);
        // Third read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 2);
        assert_eq!(buffer, [0, 0, 0, 0]);
        // Fourth read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 0);
        assert_eq!(buffer, [0, 0, 0, 0]);
    }

    #[test]
    fn test_calculate_piece_commitment() {
        use std::io::Cursor;

        let data = vec![2u8; 200];
        let cursor = Cursor::new(data.clone());

        let commitment = calculate_piece_commitment(cursor, data.len() as u64).unwrap();
        assert_eq!(
            commitment,
            [
                152, 58, 157, 235, 187, 58, 81, 61, 113, 252, 178, 149, 158, 13, 242, 24, 54, 98,
                148, 15, 250, 217, 3, 24, 152, 110, 93, 173, 117, 209, 251, 37,
            ]
        );

        // Test with zero-length data
        let empty_data = Vec::new();
        let empty_cursor = Cursor::new(empty_data);

        let empty_commitment = calculate_piece_commitment(empty_cursor, 0);
        assert!(matches!(empty_commitment, Err(CommPError::PieceTooSmall)));
    }
}
