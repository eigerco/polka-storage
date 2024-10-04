mod zero;

use cid::{multihash::Multihash, Cid};

use crate::piece::PaddedPieceSize;

/// Filecoin piece or sector data commitment merkle node/root (CommP & CommD)
///
/// https://github.com/multiformats/multicodec/blob/badcfe56bb7e0bbb06b60d57565186cd6be1f932/table.csv#L554
pub const FIL_COMMITMENT_UNSEALED: u64 = 0xf101;

/// Filecoin sector data commitment merkle node/root - sealed and replicated
/// (CommR)
///
/// https://github.com/multiformats/multicodec/blob/badcfe56bb7e0bbb06b60d57565186cd6be1f932/table.csv#L555
pub const FIL_COMMITMENT_SEALED: u64 = 0xf102;

/// SHA2-256 with the two most significant bits from the last byte zeroed (as
/// via a mask with 0b00111111) - used for proving trees as in Filecoin.
///
/// https://github.com/multiformats/multicodec/blob/badcfe56bb7e0bbb06b60d57565186cd6be1f932/table.csv#L153
pub const SHA2_256_TRUNC254_PADDED: u64 = 0x1012;

/// Poseidon using BLS12-381 and arity of 2 with Filecoin parameters
///
/// https://github.com/multiformats/multicodec/blob/badcfe56bb7e0bbb06b60d57565186cd6be1f932/table.csv#L537
pub const POSEIDON_BLS12_381_A1_FC1: u64 = 0xb401;

#[derive(Debug, Clone, Copy)]
pub enum CommitmentKind {
    // CommP - Piece commitment
    Piece,
    // CommD - Data commitment
    Data,
    // CommR - Replica commitment
    Replica,
}

impl CommitmentKind {
    fn multicodec(&self) -> u64 {
        match self {
            CommitmentKind::Piece | CommitmentKind::Data => FIL_COMMITMENT_UNSEALED,
            CommitmentKind::Replica => FIL_COMMITMENT_SEALED,
        }
    }

    fn multihash(&self) -> u64 {
        match self {
            CommitmentKind::Piece | CommitmentKind::Data => SHA2_256_TRUNC254_PADDED,
            CommitmentKind::Replica => POSEIDON_BLS12_381_A1_FC1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Commitment {
    commitment: [u8; 32],
    kind: CommitmentKind,
}

impl Commitment {
    pub fn new(commitment: [u8; 32], kind: CommitmentKind) -> Self {
        Self { commitment, kind }
    }

    pub fn from_cid(cid: &Cid, kind: CommitmentKind) -> Result<Self, &'static str> {
        let mut commitment = [0; 32];
        commitment.copy_from_slice(cid.hash().digest());

        let multicodec = cid.codec();
        let multihash = cid.hash().code();

        match kind {
            CommitmentKind::Piece | CommitmentKind::Data => {
                if multicodec != FIL_COMMITMENT_UNSEALED {
                    return Err("invalid multicodec for commitment");
                }

                if multihash != SHA2_256_TRUNC254_PADDED {
                    return Err("invalid multihash for commitment");
                }
            }
            CommitmentKind::Replica => {
                if multicodec != FIL_COMMITMENT_SEALED {
                    return Err("invalid multicodec for commitment");
                }
            }
        }

        Ok(Self { commitment, kind })
    }

    /// Returns the raw commitment bytes.
    pub fn raw(&self) -> [u8; 32] {
        self.commitment
    }

    /// Converts the commitment to a CID.
    pub fn cid(&self) -> Cid {
        let multihash = self.kind.multihash();
        let multicodec = self.kind.multicodec();
        let hash = Multihash::wrap(multihash, &self.commitment).expect("correct commitment");
        Cid::new_v1(multicodec, hash)
    }
}

/// Returns a zero-piece commitment for a given piece size.
pub fn zero_piece_commitment(size: PaddedPieceSize) -> Commitment {
    Commitment {
        commitment: zero::zero_piece_commitment(size),
        kind: CommitmentKind::Piece,
    }
}
