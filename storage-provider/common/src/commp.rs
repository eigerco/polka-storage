use std::{fs::File, io::{BufReader, Read}, path::Path};

use filecoin_hashers::{
    sha256::{Sha256Domain, Sha256Hasher},
    Domain,
};
use fr32::Fr32Reader;
use polka_storage_proofs::ZeroPaddingReader;
use primitives::{
    commitment::{piece::PaddedPieceSize, CommP, Commitment},
    NODE_SIZE,
};
use storage_proofs_core::merkle::BinaryMerkleTree;
use thiserror::Error;

/// Calculates piece commitment for the file at a path.
/// Assumes the file at path is a CARv2 archive.
pub fn commp<P: AsRef<Path>>(path: P) -> Result<(Commitment<CommP>, PaddedPieceSize), CommPError> {
    let source_file = File::open(path.as_ref())?;
    let file_size = source_file.metadata()?.len();

    let padded_piece_size = PaddedPieceSize::from_arbitrary_size(file_size as u64);
    // how many zeroes we need to add, so after Fr32 padding it'll be a power of two
    let padded_with_zeroes = *padded_piece_size.unpadded();

    let buffered = BufReader::new(source_file);
    let mut zero_padding_reader = ZeroPaddingReader::new(buffered,
        padded_with_zeroes);

    calculate_piece_commitment(&mut zero_padding_reader, padded_piece_size)
        .map(|commp| (commp, padded_piece_size))
}

/// Calculate the piece commitment for a given data source.
///
///  Reference — <https://spec.filecoin.io/systems/filecoin_files/piece/#section-systems.filecoin_files.piece.data-representation>
pub fn calculate_piece_commitment<R: Read>(
    source: R,
    piece_size: PaddedPieceSize,
) -> Result<Commitment<CommP>, CommPError> {
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
        .map_err(|err| CommPError::TreeBuild(err.to_string()))?;

    // Read and return the root of the tree
    let mut raw = [0; NODE_SIZE];
    tree.root()
        .write_bytes(&mut raw)
        .expect("destination buffer large enough");

    Ok(raw.into())
}

#[derive(Debug, Error)]
pub enum CommPError {
    #[error("Piece is not valid size: {0}")]
    InvalidPieceSize(String),
    #[error("Tree build error: {0}")]
    TreeBuild(String),
    #[error("IOError: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::{env, io::Cursor, path::Path};

    use polka_storage_proofs::ZeroPaddingReader;
    use primitives::{commitment::piece::PaddedPieceSize, sector::SectorSize};
    use filecoin_proofs::generate_piece_commitment;

    use crate::commp::commp;

    use super::calculate_piece_commitment;

    #[test]
    fn test_filecoin_commp_matches_ours() {
        let workspace_root = env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = Path::new(&workspace_root)
            .join("../..")
            .join("examples/big_file_184k.car");

        let (commitment, _) = commp(&path).unwrap();

        let f = std::fs::File::open(&path).unwrap();
        let len = f.metadata().unwrap().len();
        let size = PaddedPieceSize::from_arbitrary_size(len);

        let piece_info = generate_piece_commitment(
            ZeroPaddingReader::new(f, *size.unpadded()),
                size.unpadded().into()
        ).unwrap();

        assert_eq!(
            piece_info.commitment,
            commitment.raw(),
        );
    }

    #[test]
    fn test_calculate_piece_commitment() {
        let data_size: u64 = 200;
        let data = vec![2u8; data_size as usize];
        let cursor = Cursor::new(data.clone());
        let padded_piece_size = PaddedPieceSize::from_arbitrary_size(data_size);
        let zero_padding_reader = ZeroPaddingReader::new(cursor, *padded_piece_size);

        let commitment =
            calculate_piece_commitment(zero_padding_reader, padded_piece_size).unwrap();
        assert_eq!(
            commitment.raw(),
            [
                152, 58, 157, 235, 187, 58, 81, 61, 113, 252, 178, 149, 158, 13, 242, 24, 54, 98,
                148, 15, 250, 217, 3, 24, 152, 110, 93, 173, 117, 209, 251, 37,
            ]
        );
    }

    #[test]
    fn test_zero_piece_commitment() {
        let size = SectorSize::_2KiB;
        let padded_piece_size = PaddedPieceSize::new(size.bytes()).unwrap();
        let cursor = Cursor::new(vec![]);
        let zero_padding_reader = ZeroPaddingReader::new(cursor, *padded_piece_size);

        let commitment =
            calculate_piece_commitment(zero_padding_reader, padded_piece_size).unwrap();

        assert_eq!(
            commitment.raw(),
            [
                252, 126, 146, 130, 150, 229, 22, 250, 173, 233, 134, 178, 143, 146, 212, 74, 79,
                36, 185, 53, 72, 82, 35, 55, 106, 121, 144, 39, 188, 24, 248, 51
            ]
        );
    }
}
