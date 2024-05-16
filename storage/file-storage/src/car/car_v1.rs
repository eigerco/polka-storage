use ipld_core::{cid::Cid, codec::Codec};
use serde::{Deserialize, Serialize};
use serde_ipld_dagcbor::{codec::DagCborCodec, error::CodecError};

// Codes taken from https://github.com/multiformats/multicodec/blob/c954a787dc6a17d099653e5f90d26fbd177d2074/table.csv
const SHA2_256_CODE: u64 = 0x12;
// Raw code used because the JS CAR implementation shows an example with it
// https://github.com/ipld/js-car
const RAW_CODE: u64 = 0x55;

pub const CAR_V1_VERSION: u8 = 1;

pub type UnixFsBlock = (Cid, Vec<u8>);

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct CarV1Header {
    pub header: u8,
    pub roots: Vec<Cid>,
}

impl CarV1Header {
    fn new(roots: Vec<Cid>) -> Self {
        Self {
            header: CAR_V1_VERSION,
            roots,
        }
    }
}

impl TryFrom<&[u8]> for CarV1Header {
    type Error = CodecError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        DagCborCodec::decode_from_slice(value)
    }
}

impl TryFrom<&CarV1Header> for Vec<u8> {
    type Error = CodecError;

    fn try_from(value: &CarV1Header) -> Result<Self, Self::Error> {
        DagCborCodec::encode_to_vec(value)
    }
}

struct CarV1 {
    header: CarV1Header,
    blocks: Vec<UnixFsBlock>,
}

#[cfg(test)]
mod tests {
    use std::fs::read;

    use digest::Digest;
    use ipld_core::{
        cid::{multihash::Multihash, Cid},
        codec::Codec,
    };
    use rand::random;
    use serde_ipld_dagcbor::codec::DagCborCodec;
    use sha2::Sha256;

    use crate::car::car_v1::{RAW_CODE, SHA2_256_CODE};

    use super::CarV1Header;

    fn generate_random_multihash<H, const H_CODE: u64>() -> Multihash<64>
    where
        H: Digest,
    {
        let bytes = random::<[u8; 32]>();
        let mut hasher = H::new();
        hasher.update(&bytes);
        let hashed_bytes = hasher.finalize();
        Multihash::wrap(H_CODE, &hashed_bytes).unwrap()
    }

    #[test]
    fn roundtrip_cid_v0() {
        let multihash = generate_random_multihash::<Sha256, SHA2_256_CODE>();
        let cid = Cid::new_v0(multihash).unwrap();
        let header = CarV1Header::new(vec![cid]);
        let encoded_header = DagCborCodec::encode_to_vec(&header).unwrap();
        let decoded_header: CarV1Header = DagCborCodec::decode_from_slice(&encoded_header).unwrap();
        assert_eq!(header, decoded_header);
    }

    #[test]
    fn roundtrip_cid_v1_sha2_256() {
        let multihash = generate_random_multihash::<sha2::Sha256, SHA2_256_CODE>();
        let cid = Cid::new_v1(RAW_CODE, multihash);
        let header = CarV1Header::new(vec![cid]);
        let encoded_header = DagCborCodec::encode_to_vec(&header).unwrap();
        let decoded_header: CarV1Header = DagCborCodec::decode_from_slice(&encoded_header).unwrap();
        assert_eq!(header, decoded_header);
    }
}
