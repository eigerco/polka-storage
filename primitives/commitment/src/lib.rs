#![no_std]

pub mod commd;
pub mod piece;
mod zero;

use cid::{multihash::Multihash, Cid};
use primitives_proofs::RegisteredSealProof;

use crate::piece::PaddedPieceSize;

/// Merkle tree node size in bytes.
pub const NODE_SIZE: usize = 32;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitmentKind {
    // CommP - Piece commitment
    Piece,
    // CommD - Data commitment
    Data,
    // CommR - Replica commitment
    Replica,
}

impl CommitmentKind {
    /// Returns the [Multicodec](https://github.com/multiformats/multicodec/blob/master/table.csv) code for the commitment kind.
    fn multicodec(&self) -> u64 {
        match self {
            CommitmentKind::Piece | CommitmentKind::Data => FIL_COMMITMENT_UNSEALED,
            CommitmentKind::Replica => FIL_COMMITMENT_SEALED,
        }
    }

    /// Returns the [Multihash](https://github.com/multiformats/multicodec/blob/master/table.csv) code for the commitment kind.
    fn multihash(&self) -> u64 {
        match self {
            CommitmentKind::Piece | CommitmentKind::Data => SHA2_256_TRUNC254_PADDED,
            CommitmentKind::Replica => POSEIDON_BLS12_381_A1_FC1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commitment {
    commitment: [u8; 32],
    kind: CommitmentKind,
}

impl Commitment {
    pub fn new(commitment: [u8; 32], kind: CommitmentKind) -> Self {
        Self { commitment, kind }
    }

    /// Creates a new `Commitment` from bytes. Returns an error if the bytes
    /// passed do not represent a valid commitment.
    pub fn from_bytes(bytes: &[u8], kind: CommitmentKind) -> Result<Self, &'static str> {
        let cid = Cid::try_from(bytes).map_err(|_| "bytes not a valid cid")?;
        Self::from_cid(&cid, kind)
    }

    /// Creates a new `Commitment` from a CID. Returns an error if the CID
    /// passed does not represent a commitment kind.
    pub fn from_cid(cid: &Cid, kind: CommitmentKind) -> Result<Self, &'static str> {
        let mut commitment = [0; 32];
        commitment.copy_from_slice(cid.hash().digest());

        let multicodec = cid.codec();
        let multihash = cid.hash().code();

        match kind {
            CommitmentKind::Piece | CommitmentKind::Data => {
                if multicodec != FIL_COMMITMENT_UNSEALED {
                    return Err("invalid multicodec for piece/data commitment");
                }

                if multihash != SHA2_256_TRUNC254_PADDED {
                    return Err("invalid multihash for piece/data commitment");
                }
            }
            CommitmentKind::Replica => {
                if multicodec != FIL_COMMITMENT_SEALED {
                    return Err("invalid multicodec for replica commitment");
                }

                if multihash != POSEIDON_BLS12_381_A1_FC1 {
                    return Err("invalid multihash for replica commitment");
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
        let hash = Multihash::wrap(multihash, &self.commitment)
            .expect("multihash is large enough so it can wrap the commitment");
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

/// Return a zero data commitment for specific seal proof.
pub fn zero_data_commitment(seal_proof: RegisteredSealProof) -> Commitment {
    let size = seal_proof.sector_size().bytes();
    let size = PaddedPieceSize::new(size).expect("sector size is a valid padded size");

    Commitment {
        // Zero data commitment is same as zero piece comment of the same size.
        // That is because we have only zero is the sector that iz zeroed out.
        commitment: zero::zero_piece_commitment(size),
        kind: CommitmentKind::Data,
    }
}

#[cfg(test)]
mod tests {
    use cid::{multihash::Multihash, Cid};

    use crate::{
        Commitment, CommitmentKind, FIL_COMMITMENT_SEALED, FIL_COMMITMENT_UNSEALED,
        POSEIDON_BLS12_381_A1_FC1, SHA2_256_TRUNC254_PADDED,
    };

    fn rand_comm() -> [u8; 32] {
        rand::random::<[u8; 32]>()
    }

    #[test]
    fn comm_d_to_cid() {
        let comm = rand_comm();

        let cid = Commitment::new(comm, CommitmentKind::Data).cid();
        assert_eq!(cid.codec(), FIL_COMMITMENT_UNSEALED);
        assert_eq!(cid.hash().code(), SHA2_256_TRUNC254_PADDED);
        assert_eq!(cid.hash().digest(), comm);
    }

    #[test]
    fn cid_to_comm_d() {
        let comm = rand_comm();

        // Correct hash format
        let mh = Multihash::wrap(SHA2_256_TRUNC254_PADDED, &comm).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh);
        let commitment = Commitment::from_cid(&c, CommitmentKind::Data).unwrap();
        assert_eq!(commitment.raw(), comm);

        // Should fail with incorrect codec
        let c = Cid::new_v1(FIL_COMMITMENT_SEALED, mh);
        let commitment = Commitment::from_cid(&c, CommitmentKind::Data);
        assert!(commitment.is_err());

        // Incorrect hash format
        let mh = Multihash::wrap(0x9999, &comm).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh);
        let commitment = Commitment::from_cid(&c, CommitmentKind::Data);
        assert!(commitment.is_err());
    }

    #[test]
    fn comm_r_to_cid() {
        let comm = rand_comm();
        let cid = Commitment::new(comm, CommitmentKind::Replica).cid();

        assert_eq!(cid.codec(), FIL_COMMITMENT_SEALED);
        assert_eq!(cid.hash().code(), POSEIDON_BLS12_381_A1_FC1);
        assert_eq!(cid.hash().digest(), comm);
    }

    #[test]
    fn cid_to_comm_r() {
        let comm = rand_comm();

        // Correct hash format
        let mh = Multihash::wrap(POSEIDON_BLS12_381_A1_FC1, &comm).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_SEALED, mh);
        let commitment = Commitment::from_cid(&c, CommitmentKind::Replica).unwrap();
        assert_eq!(commitment.raw(), comm);

        // Should fail with incorrect codec
        let c = Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh);
        let commitment = Commitment::from_cid(&c, CommitmentKind::Replica);
        assert!(commitment.is_err());

        // Incorrect hash format
        let mh = Multihash::wrap(0x9999, &comm).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_SEALED, mh);
        let commitment = Commitment::from_cid(&c, CommitmentKind::Replica);
        assert!(commitment.is_err());
    }

    #[test]
    fn symmetric_conversion() {
        let comm = rand_comm();

        // piece
        let cid = Commitment::new(comm, CommitmentKind::Piece).cid();
        assert_eq!(
            Commitment::from_cid(&cid, CommitmentKind::Piece).unwrap(),
            Commitment {
                commitment: comm,
                kind: CommitmentKind::Piece
            }
        );

        // data
        let cid = Commitment::new(comm, CommitmentKind::Data).cid();
        assert_eq!(
            Commitment::from_cid(&cid, CommitmentKind::Data).unwrap(),
            Commitment {
                commitment: comm,
                kind: CommitmentKind::Data
            }
        );

        // replica
        let cid = Commitment::new(comm, CommitmentKind::Replica).cid();
        assert_eq!(
            Commitment::from_cid(&cid, CommitmentKind::Replica).unwrap(),
            Commitment {
                commitment: comm,
                kind: CommitmentKind::Replica
            }
        );
    }
}
