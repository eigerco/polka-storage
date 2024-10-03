use std::io::Read;

use filecoin_hashers::{
    sha256::{Sha256Domain, Sha256Hasher},
    Domain,
};
use fr32::Fr32Reader;
use primitives_shared::{commcid::Commitment, piece::PaddedPieceSize, NODE_SIZE};
use storage_proofs_core::merkle::BinaryMerkleTree;
use thiserror::Error;

/// Reader that returns zeros if the inner reader is empty.
pub struct ZeroPaddingReader<R: Read> {
    /// The inner reader to read from.
    inner: R,
    /// The number of bytes this 0-padding reader has left to produce.
    remaining: usize,
}

impl<R: Read> ZeroPaddingReader<R> {
    pub fn new(inner: R, total_size: usize) -> Self {
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

        // Number of bytes that the reader will produce in this execution
        let to_read = buf.len().min(self.remaining);
        // Number of bytes that we read from the inner reader
        let read = self.inner.read(&mut buf[..to_read])?;

        // If we read from the inner reader less then the required bytes, 0-pad
        // the rest of the buffer.
        if read < to_read {
            buf[read..to_read].fill(0);
        }

        // Decrease the number of bytes this 0-padding reader has left to produce.
        self.remaining -= to_read;

        // Return the number of bytes that we wrote to the buffer.
        Ok(to_read)
    }
}

/// Calculate the piece commitment for a given data source.
///
///  Reference — <https://spec.filecoin.io/systems/filecoin_files/piece/#section-systems.filecoin_files.piece.data-representation>
pub fn calculate_piece_commitment<R: Read>(
    source: R,
    piece_size: PaddedPieceSize,
) -> Result<Commitment, CommPError> {
    // This reader adds two zero bits to each 254 bits of data read from the source.
    let mut fr32_reader = Fr32Reader::new(source);

    // Buffer used for reading data used for leafs.
    let mut buffer = [0; NODE_SIZE];
    // Number of leafs
    let num_leafs = piece_size.div_ceil(NODE_SIZE as u64) as usize;

    // Elements iterator used by the MerkleTree. The elements returned by the
    // iterator represent leafs of the tree
    let elements_iterator = (0..num_leafs).map(|_| {
        fr32_reader.read_exact(&mut buffer)?;
        let hash = Sha256Domain::try_from_bytes(&buffer)?;
        Ok(hash)
    });
    let tree = BinaryMerkleTree::<Sha256Hasher>::try_from_iter(elements_iterator)
        .map_err(|err| CommPError::TreeBuildError(err.to_string()))?;

    // Read and return the root of the tree
    let mut commitment = [0; NODE_SIZE];
    tree.root()
        .write_bytes(&mut commitment)
        .expect("destination buffer large enough");

    Ok(commitment)
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

    use primitives_shared::piece::PaddedPieceSize;

    use crate::commands::utils::commp::{calculate_piece_commitment, ZeroPaddingReader};

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

        let data_size: usize = 200;
        let data = vec![2u8; data_size];
        let cursor = Cursor::new(data.clone());
        let padded_piece_size = PaddedPieceSize::new(data_size.next_power_of_two() as u64).unwrap();
        let zero_padding_reader = ZeroPaddingReader::new(cursor, *padded_piece_size as usize);

        let commitment =
            calculate_piece_commitment(zero_padding_reader, padded_piece_size).unwrap();
        assert_eq!(
            commitment,
            [
                152, 58, 157, 235, 187, 58, 81, 61, 113, 252, 178, 149, 158, 13, 242, 24, 54, 98,
                148, 15, 250, 217, 3, 24, 152, 110, 93, 173, 117, 209, 251, 37,
            ]
        );
    }
}
