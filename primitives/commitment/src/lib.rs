#![cfg_attr(not(feature = "std"), no_std)] // no_std by default, requires "std" for std-support

pub mod commd;
pub mod piece;
mod zero;

use core::{fmt::Display, marker::PhantomData};

use cid::{multihash::Multihash, Cid};
use primitives_proofs::RegisteredSealProof;
use sealed::sealed;

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

#[sealed]
pub trait CommitmentKind {
    /// Returns the [Multicodec](https://github.com/multiformats/multicodec/blob/master/table.csv) code for the commitment kind.
    fn multicodec() -> u64;
    /// Returns the [Multihash](https://github.com/multiformats/multicodec/blob/master/table.csv) code for the commitment kind.
    fn multihash() -> u64;
}

/// Data commitment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommD;

#[sealed]
impl CommitmentKind for CommD {
    fn multicodec() -> u64 {
        FIL_COMMITMENT_UNSEALED
    }

    fn multihash() -> u64 {
        SHA2_256_TRUNC254_PADDED
    }
}

/// Piece commitment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommP;

#[sealed]
impl CommitmentKind for CommP {
    fn multicodec() -> u64 {
        FIL_COMMITMENT_UNSEALED
    }

    fn multihash() -> u64 {
        SHA2_256_TRUNC254_PADDED
    }
}

/// Replica commitment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommR;

#[sealed]
impl CommitmentKind for CommR {
    fn multicodec() -> u64 {
        FIL_COMMITMENT_SEALED
    }

    fn multihash() -> u64 {
        POSEIDON_BLS12_381_A1_FC1
    }
}

// TODO: Implement TypeInfo for this type so we can use it in pallets.
#[derive(thiserror::Error)]
pub enum CommitmentError {
    #[error("bytes not a valid cid")]
    InvalidCidBytes,
    #[error("invalid multicodec for commitment")]
    InvalidMultiCodec,
    #[error("invalid multihash for commitment")]
    InvalidMultiHash,
}

impl core::fmt::Debug for CommitmentError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commitment<Kind>
where
    Kind: CommitmentKind,
{
    raw: [u8; 32],
    kind: PhantomData<Kind>,
}

impl<Kind> Commitment<Kind>
where
    Kind: CommitmentKind,
{
    /// Creates a new `Commitment` from bytes of a valid CID. Returns an error
    /// if the bytes passed do not represent a valid commitment.
    pub fn from_cid_bytes(bytes: &[u8]) -> Result<Self, CommitmentError> {
        let cid = Cid::try_from(bytes).map_err(|_| CommitmentError::InvalidCidBytes)?;
        Self::from_cid(&cid)
    }

    /// Creates a new `Commitment` from a CID. Returns an error if the CID
    /// passed does not represent a commitment kind.
    pub fn from_cid(cid: &Cid) -> Result<Self, CommitmentError> {
        let mut raw = [0; 32];
        raw.copy_from_slice(cid.hash().digest());

        // Check multicodec of the cid
        if cid.codec() != Kind::multicodec() {
            return Err(CommitmentError::InvalidMultiCodec);
        }

        // Check multihash of the cid
        if cid.hash().code() != Kind::multihash() {
            return Err(CommitmentError::InvalidMultiHash);
        }

        Ok(Self {
            raw,
            kind: PhantomData,
        })
    }

    /// Returns the raw commitment bytes.
    pub fn raw(&self) -> [u8; 32] {
        self.raw
    }

    /// Converts the commitment to a CID.
    pub fn cid(&self) -> Cid {
        let multihash = Kind::multihash();
        let multicodec = Kind::multicodec();
        let hash = Multihash::wrap(multihash, &self.raw)
            .expect("multihash is large enough so it can wrap the commitment");
        Cid::new_v1(multicodec, hash)
    }
}

impl<Kind> Display for Commitment<Kind>
where
    Kind: CommitmentKind,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.cid())
    }
}

impl<Kind> TryFrom<Cid> for Commitment<Kind>
where
    Kind: CommitmentKind,
{
    type Error = CommitmentError;

    fn try_from(value: Cid) -> Result<Self, Self::Error> {
        Self::from_cid(&value)
    }
}

impl<Kind> From<[u8; 32]> for Commitment<Kind>
where
    Kind: CommitmentKind,
{
    fn from(value: [u8; 32]) -> Self {
        Self {
            raw: value,
            kind: PhantomData,
        }
    }
}

impl Commitment<CommP> {
    /// Returns a zero piece commitment for a given piece size.
    pub fn zero(size: PaddedPieceSize) -> Self {
        Commitment::from(zero::zero_piece_commitment(size))
    }
}

impl Commitment<CommD> {
    /// Return a zero data commitment for specific seal proof.
    pub fn zero(seal_proof: RegisteredSealProof) -> Self {
        let size = seal_proof.sector_size().bytes();
        let size = PaddedPieceSize::new(size).expect("sector size is a valid padded size");

        // Zero data commitment is the same as zero piece commitment of the
        // same size.
        Commitment::from(zero::zero_piece_commitment(size))
    }
}

#[cfg(test)]
mod tests {
    use core::marker::PhantomData;

    use cid::{multihash::Multihash, Cid};

    use crate::{
        CommD, CommP, CommR, Commitment, FIL_COMMITMENT_SEALED, FIL_COMMITMENT_UNSEALED,
        POSEIDON_BLS12_381_A1_FC1, SHA2_256_TRUNC254_PADDED,
    };

    fn rand_comm() -> [u8; 32] {
        rand::random::<[u8; 32]>()
    }

    #[test]
    fn comm_d_to_cid() {
        let raw = rand_comm();

        let cid = Commitment::<CommD>::from(raw).cid();
        assert_eq!(cid.codec(), FIL_COMMITMENT_UNSEALED);
        assert_eq!(cid.hash().code(), SHA2_256_TRUNC254_PADDED);
        assert_eq!(cid.hash().digest(), raw);
    }

    #[test]
    fn cid_to_comm_d() {
        let raw = rand_comm();

        // Correct hash format
        let mh = Multihash::wrap(SHA2_256_TRUNC254_PADDED, &raw).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh);
        let commitment = Commitment::<CommD>::from_cid(&c).unwrap();
        assert_eq!(commitment.raw(), raw);

        // Should fail with incorrect codec
        let c = Cid::new_v1(FIL_COMMITMENT_SEALED, mh);
        let commitment = Commitment::<CommD>::from_cid(&c);
        assert!(commitment.is_err());

        // Incorrect hash format
        let mh = Multihash::wrap(0x9999, &raw).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh);
        let commitment = Commitment::<CommD>::from_cid(&c);
        assert!(commitment.is_err());
    }

    #[test]
    fn comm_r_to_cid() {
        let comm = rand_comm();
        let cid = Commitment::<CommR>::from(comm).cid();

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
        let commitment = Commitment::<CommR>::from_cid(&c).unwrap();
        assert_eq!(commitment.raw(), comm);

        // Should fail with incorrect codec
        let c = Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh);
        let commitment = Commitment::<CommR>::from_cid(&c);
        assert!(commitment.is_err());

        // Incorrect hash format
        let mh = Multihash::wrap(0x9999, &comm).unwrap();
        let c = Cid::new_v1(FIL_COMMITMENT_SEALED, mh);
        let commitment = Commitment::<CommR>::from_cid(&c);
        assert!(commitment.is_err());
    }

    #[test]
    fn symmetric_conversion() {
        let raw = rand_comm();

        // piece
        let cid = Commitment::<CommP>::from(raw).cid();
        assert_eq!(
            Commitment::<CommP>::from_cid(&cid).unwrap(),
            Commitment::<CommP> {
                raw,
                kind: PhantomData
            }
        );

        // data
        let cid = Commitment::<CommD>::from(raw).cid();
        assert_eq!(
            Commitment::<CommD>::from_cid(&cid).unwrap(),
            Commitment::<CommD> {
                raw,
                kind: PhantomData
            }
        );

        // replica
        let cid = Commitment::<CommR>::from(raw).cid();
        assert_eq!(
            Commitment::<CommR>::from_cid(&cid).unwrap(),
            Commitment::<CommR> {
                raw,
                kind: PhantomData
            }
        );
    }
}
