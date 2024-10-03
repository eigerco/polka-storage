use cid::{multihash::Multihash, Cid};

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

/// Type representing a 32-byte commitment.
pub type Commitment = [u8; 32];

/// Converts a commitment to a CID.
pub fn commitment_to_cid(
    multicodec: u64,
    multihash: u64,
    commitment: Commitment,
) -> Result<Cid, &'static str> {
    validate_cid_segments(multicodec, multihash, &commitment)?;

    let hash =
        Multihash::wrap(multihash, &commitment).map_err(|_| "failed to wrap commitment cid")?;
    Ok(Cid::new_v1(multicodec, hash))
}

/// Destructure a CID to a commitment.
pub fn cid_to_commitment(cid: &Cid) -> Result<(u64, u64, Commitment), &'static str> {
    validate_cid_segments(cid.codec(), cid.hash().code(), cid.hash().digest())?;

    let mut comm = Commitment::default();
    comm.copy_from_slice(cid.hash().digest());

    Ok((cid.codec(), cid.hash().code(), comm))
}

/// Converts a piece commitment to a CID.
pub fn piece_commitment_to_cid(comm_p: Commitment) -> Result<Cid, &'static str> {
    commitment_to_cid(FIL_COMMITMENT_UNSEALED, SHA2_256_TRUNC254_PADDED, comm_p)
}

/// Converts a data commitment to a CID.
pub fn data_commitment_to_cid(comm_d: Commitment) -> Result<Cid, &'static str> {
    commitment_to_cid(FIL_COMMITMENT_UNSEALED, SHA2_256_TRUNC254_PADDED, comm_d)
}

/// Returns an error if the provided CID parts conflict with each other.
///
/// Reference:
fn validate_cid_segments(
    multicodec: u64,
    multihash: u64,
    commitment: &[u8],
) -> Result<(), &'static str> {
    match multicodec {
        FIL_COMMITMENT_UNSEALED => {
            if multihash != SHA2_256_TRUNC254_PADDED {
                return Err("Incorrect hash function for unsealed commitment");
            }
        }
        FIL_COMMITMENT_SEALED => {
            if multihash != POSEIDON_BLS12_381_A1_FC1 {
                return Err("Incorrect hash function for sealed commitment");
            }
        }
        _ => return Err("Invalid Codec, expected sealed or unsealed commitment codec"),
    }

    if commitment.len() != 32 {
        Err("commitments must be 32 bytes long")
    } else {
        Ok(())
    }
}
