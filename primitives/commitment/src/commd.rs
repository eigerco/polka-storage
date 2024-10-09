extern crate alloc;
use alloc::vec::Vec;
use core::ops::Deref;

use primitives_proofs::SectorSize;
use sha2::{Digest, Sha256};

use crate::{
    piece::{PaddedPieceSize, PieceInfo, UnpaddedPieceSize},
    zero_piece_commitment, Commitment, CommitmentKind, NODE_SIZE,
};

// Ensure that the pieces are correct sizes
fn ensure_piece_sizes(
    sector_size: SectorSize,
    piece_infos: &[PieceInfo],
) -> Result<(), CommDError> {
    // Sector should be able to hold all pieces
    let size_sum = piece_infos.iter().map(|piece| *piece.size).sum::<u64>();
    if size_sum > sector_size.bytes() {
        return Err(CommDError::PieceSizeTooLarge);
    }

    // Check if there are too many pieces for a sector of this size
    let sector_size = PaddedPieceSize::new(sector_size.bytes()).unwrap();
    let num_of_pieces = piece_infos.len() as u64;
    let max_pieces = *sector_size.unpadded() / *UnpaddedPieceSize::MIN;
    if num_of_pieces > max_pieces {
        return Err(CommDError::TooManyPieces);
    }

    Ok(())
}

/// Computes an unsealed sector CID (CommD) from its constituent piece CIDs (CommPs) and sizes.
pub fn compute_unsealed_sector_commitment(
    sector_size: SectorSize,
    piece_infos: &[PieceInfo],
) -> Result<Commitment, CommDError> {
    let padded_sector_size = PaddedPieceSize::new(sector_size.bytes()).unwrap();

    // In case of no pieces, return the piece zero commitment for the whole
    // sector size.
    if piece_infos.is_empty() {
        return Ok(zero_piece_commitment(padded_sector_size));
    }

    // Check if pieces are correct sizes.
    ensure_piece_sizes(sector_size, piece_infos)?;

    // Reduce the pieces to the 1-piece commitment
    let mut reduction = CommDPieceReduction::new();
    reduction.add_pieces(piece_infos.iter().copied());
    let commitment = reduction.finish().expect("at least one piece was added");

    Ok(commitment)
}

/// Reduces pieces passed to their data commitment. The process of the reduction
/// is following:
///
/// 1. Pieces are added to the stack one by one.
/// 2. After each piece is added, the stack is reduced by combining pieces of
///    the same size.
/// 3. If a piece to be added is larger than the last piece on the stack,
///    padding pieces are added until the last piece on the stack is at least as
///    large as the piece to be added.
/// 4. The process continues until all pieces have been added and reduced.
/// 5. At the end, if there is more than one piece on the stack, padding pieces
///    are added until the stack can be reduced to a single piece.
/// 6. The final single piece represents the data commitment for all the input
///    pieces.
struct CommDPieceReduction {
    /// Pieces stack
    pieces: Vec<PieceInfo>,
}

impl CommDPieceReduction {
    fn new() -> Self {
        CommDPieceReduction { pieces: Vec::new() }
    }

    // Add many pieces
    fn add_pieces<P>(&mut self, pieces: P)
    where
        P: Iterator<Item = PieceInfo>,
    {
        pieces.for_each(|p| self.add_piece(p));
    }

    // Add a single piece
    fn add_piece(&mut self, piece: PieceInfo) {
        // Handle first piece
        if self.pieces.is_empty() {
            self.pieces.push(piece);
            return;
        }

        // Add padding pieces to the stack until we reduce the current pieces to
        // the size that is equal to the new piece. With this we achieve that
        // the new piece will be reduced to a single piece after adding it to
        // the stack. Will always iterate at least once since if it was empty
        // the first condition would have triggered and returned.
        while let Some(last_piece) = self.pieces.last() {
            let last_added_piece_size = last_piece.size;
            // We can stop stop adding padding pieces if the last added padding
            // piece is the same size as the actual piece.
            if last_added_piece_size.deref() >= piece.size.deref() {
                break;
            }

            let padding_piece = padding_piece(last_added_piece_size);
            self.pieces.push(padding_piece);

            // We need to reduce the pieces before the next iteration. Because
            // we are always adding the padding to the last piece. And the last
            // piece changes based on the result of reduction.
            self.reduce();
        }

        // Add the new piece to the stack
        self.pieces.push(piece);

        // Reduce the pieces
        self.reduce();
    }

    /// Combine pieces until there are any on the stack available to combine
    fn reduce(&mut self) {
        loop {
            // If there is only a single piece on the stack we break the loop
            let pieces_len = self.pieces.len();
            if pieces_len < 2 {
                break;
            }

            // If the two pieces on top of the stack are not the same size, we
            // can't reduce them
            let last_piece_size = self.pieces[pieces_len - 1].size;
            let second_last_piece_size = self.pieces[pieces_len - 2].size;
            if last_piece_size != second_last_piece_size {
                break;
            }

            // Pop and join the two pieces on top of the stack. Push the
            // combined piece back to the stack
            let last_piece = self
                .pieces
                .pop()
                .expect("we know there are at least two pieces");
            let second_last_piece = self
                .pieces
                .pop()
                .expect("we know there are at least two pieces");
            let joined =
                join_piece_infos(second_last_piece, last_piece).expect("pieces are the same size");
            self.pieces.push(joined);
        }
    }

    /// Finish the reduction of all pieces. Result is a data commitment for the
    /// pieces added.
    fn finish(mut self) -> Option<Commitment> {
        // Check if we still have more then one piece on the stack. If we do,
        // that means that we should add some additional padding pieces at the
        // end until we can reduce them to a single piece
        while self.pieces.len() > 1 {
            let last_added_piece_size = self.pieces.last().expect("at least one piece exists").size;
            self.pieces.push(padding_piece(last_added_piece_size));
            self.reduce();
        }

        // Finally a single piece with the commitment that represents all
        // reduced pieces
        Some(self.pieces.pop()?.commitment)
    }
}

/// Create a piece of specific size used as a padding.
fn padding_piece(piece_size: PaddedPieceSize) -> PieceInfo {
    PieceInfo {
        commitment: zero_piece_commitment(piece_size),
        size: piece_size,
    }
}

/// Join two equally sized `PieceInfo`s together, by hashing them and adding
/// their sizes.
fn join_piece_infos(left: PieceInfo, right: PieceInfo) -> Result<PieceInfo, CommDError> {
    // The pieces passed should be same size
    if left.size != right.size {
        return Err(CommDError::InvalidPieceSize);
    }

    let hash = piece_hash(&left.commitment.raw(), &right.commitment.raw());
    let mut comm = [0; 32];
    comm.copy_from_slice(&hash);

    let size = left.size + right.size;

    Ok(PieceInfo {
        commitment: Commitment::new(comm, CommitmentKind::Piece),
        size,
    })
}

/// Calculate Hash of two raw piece commitments
pub fn piece_hash(a: &[u8], b: &[u8]) -> [u8; 32] {
    let mut buf = [0u8; NODE_SIZE * 2];
    buf[..NODE_SIZE].copy_from_slice(a);
    buf[NODE_SIZE..].copy_from_slice(b);

    let mut hashed = Sha256::digest(buf);

    // strip last two bits, to ensure result is in Fr.
    hashed[31] &= 0b0011_1111;

    hashed.into()
}

#[derive(Debug)]
pub enum CommDError {
    InvalidPieceSize,
    PieceSizeTooLarge,
    TooManyPieces,
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use core::str::FromStr;

    use cid::Cid;
    use primitives_proofs::SectorSize;

    use super::*;

    #[test]
    fn test_compute_comm_d_empty() {
        let comm_d = compute_unsealed_sector_commitment(SectorSize::_2KiB, &[])
            .expect("failed to verify pieces, empty piece infos");
        assert_eq!(
            comm_d.raw(),
            [
                252, 126, 146, 130, 150, 229, 22, 250, 173, 233, 134, 178, 143, 146, 212, 74, 79,
                36, 185, 53, 72, 82, 35, 55, 106, 121, 144, 39, 188, 24, 248, 51
            ]
        );
    }

    /// Reference: <https://github.com/ChainSafe/fil-actor-states/blob/9a508dbdd5d5049b135fbf908caa6cf18007a208/fil_actors_shared/src/abi/commp.rs#L145>
    #[test]
    fn compute_unsealed_sector_cid() {
        let pieces = [
            (
                Some("baga6ea4seaqknzm22isnhsxt2s4dnw45kfywmhenngqq3nc7jvecakoca6ksyhy"),
                256 << 20,
            ),
            (
                Some("baga6ea4seaqnq6o5wuewdpviyoafno4rdpqnokz6ghvg2iyeyfbqxgcwdlj2egi"),
                1024 << 20,
            ),
            (
                Some("baga6ea4seaqpixk4ifbkzato3huzycj6ty6gllqwanhdpsvxikawyl5bg2h44mq"),
                512 << 20,
            ),
            (
                Some("baga6ea4seaqaxwe5dy6nt3ko5tngtmzvpqxqikw5mdwfjqgaxfwtzenc6bgzajq"),
                512 << 20,
            ),
            (
                Some("baga6ea4seaqpy33nbesa4d6ot2ygeuy43y4t7amc4izt52mlotqenwcmn2kyaai"),
                1024 << 20,
            ),
            (
                Some("baga6ea4seaqphvv4x2s2v7ykgc3ugs2kkltbdeg7icxstklkrgqvv72m2v3i2aa"),
                256 << 20,
            ),
            (
                Some("baga6ea4seaqf5u55znk6jwhdsrhe37emzhmehiyvjxpsww274f6fiy3h4yctady"),
                512 << 20,
            ),
            (
                Some("baga6ea4seaqa3qbabsbmvk5er6rhsjzt74beplzgulthamm22jue4zgqcuszofi"),
                1024 << 20,
            ),
            (
                Some("baga6ea4seaqiekvf623muj6jpxg6vsqaikyw3r4ob5u7363z7zcaixqvfqsc2ji"),
                256 << 20,
            ),
            (
                Some("baga6ea4seaqhsewv65z2d4m5o4vo65vl5o6z4bcegdvgnusvlt7rao44gro36pi"),
                512 << 20,
            ),
            // The sector has to be filled entirely, before we can calculate the
            // commitment, so we add two more empty pieces here.
            (None, 8 << 30),
            (None, 16 << 30),
        ];

        let pieces = pieces
            .into_iter()
            .map(|(cid, size)| {
                let size = PaddedPieceSize::new(size).unwrap();
                let commitment = match cid {
                    Some(cid) => {
                        let cid = Cid::from_str(cid).unwrap();
                        Commitment::from_cid(&cid, CommitmentKind::Piece).unwrap()
                    }
                    None => zero_piece_commitment(size),
                };

                PieceInfo { commitment, size }
            })
            .collect::<Vec<_>>();

        let comm_d = compute_unsealed_sector_commitment(SectorSize::_32GiB, &pieces).unwrap();
        let cid = comm_d.cid();

        assert_eq!(
            cid.to_string(),
            "baga6ea4seaqiw3gbmstmexb7sqwkc5r23o3i7zcyx5kr76pfobpykes3af62kca"
        );
    }
}
