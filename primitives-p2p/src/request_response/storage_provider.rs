use cid::Cid;

pub const SP_REQUEST_RESPONSE_PROTOCOL: &str = "/polka-storage-provider-req-resp/1.0.0";

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct PieceInfoRequest {
    pub piece_cid: Cid,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub enum PieceInfoResponse {
    Found(PieceInfo),
    NotFound(Cid),
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct PieceInfo {
    pub roots: Vec<Cid>,
}
