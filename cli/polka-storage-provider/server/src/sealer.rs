use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use filecoin_proofs::{
    add_piece, PaddedBytesAmount, PieceInfo as FcPieceInfo, UnpaddedBytesAmount,
};
use polka_storage_proofs::porep::{sealer::filler_pieces, PoRepError};
use polka_storage_provider_common::commp::ZeroPaddingReader;
use primitives_commitment::piece::PaddedPieceSize;
use primitives_proofs::{RawCommitment, SectorSize};

#[derive(Debug, thiserror::Error)]
pub enum SealerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    PoRep(#[from] PoRepError),
}

/// Prepares an arbitrary piece to be used by [`create_sector`].
///
/// It does so by calculating the proper size for the padded reader
/// (by means of converting the raw size into a padded size and then into an unpadded size),
/// and then by wrapping the respective file reader with a [`ZeroPaddingReader`].
pub fn prepare_piece<P>(
    piece_path: P,
    piece_comm_p: RawCommitment,
) -> Result<(ZeroPaddingReader<File>, FcPieceInfo), std::io::Error>
where
    P: AsRef<Path>,
{
    let piece_file = File::open(piece_path)?;
    let piece_raw_size = piece_file.metadata()?.len();

    // If a file is unpadded, we can calculate its final size with Fr32 Padding and next power of two padding via
    // `PaddedPieceSize::from_arbitrary_size`. E.g. 900 bytes -> 1024 bytes. However, Filecoin's `add_piece` methods
    // requires size, to be before `Fr32` padding, so we call `.unpadded()` to get the `Fr32 unpadded`.
    // Required because of Filecoin magic, we'll probably need to change our Unpadded/Padded
    // into Filecoin implementations and instead write extensions for them to make them ergonomic
    let piece_padded_unpadded_length =
        PaddedPieceSize::from_arbitrary_size(piece_raw_size).unpadded();
    let piece_padded_file = ZeroPaddingReader::new(piece_file, *piece_padded_unpadded_length);

    let piece_info = FcPieceInfo {
        commitment: piece_comm_p,
        size: UnpaddedBytesAmount(*piece_padded_unpadded_length),
    };

    Ok((piece_padded_file, piece_info))
}

/// Create a sector from several pieces. The resulting sector will be written into `sector_writer`.
pub fn create_sector<PieceReader, SectorWriter>(
    pieces: Vec<(PieceReader, FcPieceInfo)>,
    mut sector_writer: SectorWriter,
    sector_size: SectorSize,
) -> Result<Vec<FcPieceInfo>, SealerError>
where
    PieceReader: Read,
    SectorWriter: Write,
{
    if pieces.len() == 0 {
        return Err(SealerError::PoRep(PoRepError::EmptySector));
    }

    let mut result_pieces: Vec<FcPieceInfo> = Vec::with_capacity(pieces.len());
    let mut piece_lengths: Vec<UnpaddedBytesAmount> = Vec::with_capacity(pieces.len());
    let mut unpadded_occupied_space: UnpaddedBytesAmount = UnpaddedBytesAmount(0);

    for (idx, (reader, piece)) in pieces.into_iter().enumerate() {
        let piece: FcPieceInfo = piece.into();
        let (calculated_piece_info, written_bytes) =
            add_piece(reader, &mut sector_writer, piece.size, &piece_lengths)?;

        piece_lengths.push(piece.size);

        // We need to add `written_bytes` not `piece.size`, as `add_piece` adds padding.
        unpadded_occupied_space = unpadded_occupied_space + written_bytes;

        if piece.commitment != calculated_piece_info.commitment {
            return Err(PoRepError::InvalidPieceCid(
                idx,
                piece.commitment,
                calculated_piece_info.commitment,
            ))?;
        }

        result_pieces.push(piece.into());
    }

    let sector_size: UnpaddedBytesAmount = PaddedBytesAmount(sector_size.bytes()).into();
    let padding_pieces = filler_pieces(sector_size - unpadded_occupied_space);
    result_pieces.extend(padding_pieces.into_iter().map(Into::into));

    Ok(result_pieces)
}
