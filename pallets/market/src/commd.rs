extern crate alloc;
use alloc::vec::Vec;

use cid::Cid;
use primitives_proofs::SectorSize;
use primitives_shared::{commitment::Commitment, piece::PaddedPieceSize, NODE_SIZE};
use sha2::{Digest, Sha256};
use primitives_shared::commitment::CommitmentKind;
use primitives_shared::commitment::zero_piece_commitment;

// Ensure that the pieces are correct sizes
fn ensure_piece_sizes(
    sector_size: SectorSize,
    piece_infos: &[PieceInfo],
) -> Result<(), CommDError> {
    let size_sum = piece_infos.iter().map(|piece| *piece.size).sum::<u64>();

    if size_sum > sector_size.bytes() {
        return Err(CommDError::PieceSizeTooLarge);
    }

    Ok(())
}

/// Computes an unsealed sector CID (CommD) from its constituent piece CIDs (CommPs) and sizes.
pub fn compute_unsealed_sector_commitment(
    sector_size: SectorSize,
    piece_infos: &[PieceInfo],
) -> Result<Commitment, CommDError> {
    let padded_sector_size = PaddedPieceSize::new(sector_size.bytes()).unwrap();
    let unpadded_sector_size = padded_sector_size.unpadded();

    // In case of no pieces, return the zero commitment for the whole sector.
    if piece_infos.is_empty() {
        let comm =
        return Ok(zero_piece_commitment(padded_sector_size));
    }

    // Check if pieces are correct sizes.
    ensure_piece_sizes(sector_size, piece_infos)?;

    todo!();

    // let commd = piece_reduction.commitment().unwrap();
    // Ok(commd)
}

#[derive(Debug, Clone, Copy)]
pub struct PieceInfo {
    /// Piece commitment
    pub commitment: Commitment,
    /// Piece size
    pub size: PaddedPieceSize,
}

/// Stack used for piece reduction.
/// TODO: Temporary implementation copied from the filecoin
struct Stack(Vec<PieceInfo>);

impl Stack {
    /// Creates a new stack.
    fn new() -> Self {
        Stack(Vec::new())
    }

    /// Pushes a single element onto the stack.
    fn shift(&mut self, el: PieceInfo) {
        self.0.push(el)
    }

    /// Look at the last element of the stack.
    fn peek(&self) -> &PieceInfo {
        &self.0[self.0.len() - 1]
    }

    /// Look at the second to last element of the stack.
    fn peek2(&self) -> &PieceInfo {
        &self.0[self.0.len() - 2]
    }

    /// Pop the last element of the stack.
    fn pop(&mut self) -> PieceInfo {
        self.0.pop().unwrap()
    }

    fn reduce1(&mut self) -> bool {
        if self.len() < 2 {
            return false;
        }

        if self.peek().size == self.peek2().size {
            let right = self.pop();
            let left = self.pop();
            let joined = join_piece_infos(left, right).unwrap();
            self.shift(joined);
            return true;
        }

        false
    }

    fn reduce(&mut self) {
        while self.reduce1() {}
    }

    fn shift_reduce(&mut self, piece: PieceInfo) {
        self.shift(piece);
        self.reduce()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

/// Join two equally sized `PieceInfo`s together, by hashing them and adding
/// their sizes.
fn join_piece_infos(left: PieceInfo, right: PieceInfo) -> Result<PieceInfo, CommDError> {
    if left.size != right.size {
        return Err(CommDError::InvalidPieceSize);
    }

    let hash = piece_hash(&left.commitment.raw(), &right.commitment.raw());
    let mut comm = [0; 32];
    comm.copy_from_slice(&hash);

    let size = left.size + right.size;

    Ok(PieceInfo {
        commitment: Commitment::new(comm, CommitmentKind::Piece),
        size
    })
}

/// Calculate Hash of two 32-byte arrays.
pub fn piece_hash(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut buf = [0u8; NODE_SIZE * 2];
    buf[..NODE_SIZE].copy_from_slice(a);
    buf[NODE_SIZE..].copy_from_slice(b);

    let mut hasher = Sha256::new();
    hasher.update(buf);
    hasher.finalize().to_vec()
}

#[derive(Debug)]
pub enum CommDError {
    PieceSizeTooLarge,
    InvalidPieceSize,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

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

        let comm_d = zero_piece_commitment(PaddedPieceSize::new(2048).unwrap());
        assert_eq!(
            comm_d.raw(),
            [
                252, 126, 146, 130, 150, 229, 22, 250, 173, 233, 134, 178, 143, 146, 212, 74, 79,
                36, 185, 53, 72, 82, 35, 55, 106, 121, 144, 39, 188, 24, 248, 51
            ]
        );

        let comm_d = zero_piece_commitment(PaddedPieceSize::new(128).unwrap());
        assert_eq!(
            comm_d.raw(),
            [
                55, 49, 187, 153, 172, 104, 159, 102, 238, 245, 151, 62, 74, 148, 218, 24, 143, 77,
                220, 174, 88, 7, 36, 252, 111, 63, 214, 13, 253, 72, 131, 51
            ]
        );
    }

    /// Reference: <https://github.com/ChainSafe/fil-actor-states/blob/9a508dbdd5d5049b135fbf908caa6cf18007a208/fil_actors_shared/src/abi/commp.rs#L145>
    #[test]
    fn compute_unsealed_sector_cid() {
        let pieces = vec![
            (Some("baga6ea4seaqknzm22isnhsxt2s4dnw45kfywmhenngqq3nc7jvecakoca6ksyhy"), 256 << 20),
            (Some("baga6ea4seaqnq6o5wuewdpviyoafno4rdpqnokz6ghvg2iyeyfbqxgcwdlj2egi"), 1024 << 20),
            (Some("baga6ea4seaqpixk4ifbkzato3huzycj6ty6gllqwanhdpsvxikawyl5bg2h44mq"), 512 << 20),
            (Some("baga6ea4seaqaxwe5dy6nt3ko5tngtmzvpqxqikw5mdwfjqgaxfwtzenc6bgzajq"), 512 << 20),
            (Some("baga6ea4seaqpy33nbesa4d6ot2ygeuy43y4t7amc4izt52mlotqenwcmn2kyaai"), 1024 << 20),
            (Some("baga6ea4seaqphvv4x2s2v7ykgc3ugs2kkltbdeg7icxstklkrgqvv72m2v3i2aa"), 256 << 20),
            (Some("baga6ea4seaqf5u55znk6jwhdsrhe37emzhmehiyvjxpsww274f6fiy3h4yctady"), 512 << 20),
            (Some("baga6ea4seaqa3qbabsbmvk5er6rhsjzt74beplzgulthamm22jue4zgqcuszofi"), 1024 << 20),
            (Some("baga6ea4seaqiekvf623muj6jpxg6vsqaikyw3r4ob5u7363z7zcaixqvfqsc2ji"), 256 << 20),
            (Some("baga6ea4seaqhsewv65z2d4m5o4vo65vl5o6z4bcegdvgnusvlt7rao44gro36pi"), 512 << 20),
            // The sector has to be filled entirely, before we can calculate the
            // commitment, so we add two more empty pieces here.
            (None, 8 << 30),
            (None, 16 << 30)
        ];

        let pieces = pieces.into_iter().map(|(cid, size)| {
            let size = PaddedPieceSize::new(size).unwrap();
            let commitment = match cid {
                Some(cid) => {
                    let cid = Cid::from_str(cid).unwrap();
                    Commitment::from_cid(&cid, CommitmentKind::Piece).unwrap()
                },
                None => zero_piece_commitment(size),
            };

            PieceInfo {
                commitment,
                size,
            }
        }).collect::<Vec<_>>();

        let comm_d = compute_unsealed_sector_commitment(SectorSize::_32GiB, &pieces).unwrap();
        let cid = comm_d.cid();

        assert_eq!(
            cid.to_string(),
            "baga6ea4seaqiw3gbmstmexb7sqwkc5r23o3i7zcyx5kr76pfobpykes3af62kca"
        );
    }
}
