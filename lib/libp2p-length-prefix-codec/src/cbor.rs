//! Based on Libp2p's request-response CBOR codec and the Universal Connectivity example:
//! * <https://github.com/libp2p/universal-connectivity/blob/b83b67f275a610101cb7ecd8aa3934094051d731/rust-peer/src/protocol.rs>
//! * <https://github.com/libp2p/rust-libp2p/blob/36546858199e25f767b7e11c5d10af2f946a5c30/protocols/request-response/src/cbor.rs>

// NOTE(@jmg-duarte,12/03/2025): this can probably be abstracted as a wrapper over any protocol
use std::{collections::TryReserveError, convert::Infallible, io, marker::PhantomData};

use async_trait::async_trait;
use cbor4ii::core::error::DecodeError;
use futures::{AsyncRead, AsyncWrite};
use libp2p::{request_response::Codec, StreamProtocol};
use serde::{de::DeserializeOwned, Serialize};

use crate::{read_length_prefixed, write_length_prefixed};

const MAX_SIZE: usize = 1024 * 1024;

/// Length-prefix CBOR codec.
pub struct LpCbor<Req, Resp> {
    phantom: PhantomData<(Req, Resp)>,
}

impl<Req, Resp> Default for LpCbor<Req, Resp> {
    fn default() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<Req, Resp> Clone for LpCbor<Req, Resp> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[async_trait]
impl<Req, Resp> Codec for LpCbor<Req, Resp>
where
    Req: Send + Serialize + DeserializeOwned,
    Resp: Send + Serialize + DeserializeOwned,
{
    type Protocol = StreamProtocol;
    type Request = Req;
    type Response = Resp;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, MAX_SIZE).await?;
        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        cbor4ii::serde::from_slice(vec.as_slice()).map_err(decode_into_io_error)
    }

    /// Reads a response from the given I/O stream according to the
    /// negotiated protocol.
    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, MAX_SIZE).await?;
        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        cbor4ii::serde::from_slice(vec.as_slice()).map_err(decode_into_io_error)
    }

    /// Writes a request to the given I/O stream according to the
    /// negotiated protocol.
    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data: Vec<u8> =
            cbor4ii::serde::to_vec(Vec::new(), &req).map_err(encode_into_io_error)?;
        write_length_prefixed(io, data).await
    }

    /// Writes a response to the given I/O stream according to the
    /// negotiated protocol.
    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data: Vec<u8> =
            cbor4ii::serde::to_vec(Vec::new(), &res).map_err(encode_into_io_error)?;
        write_length_prefixed(io, data).await
    }
}

fn decode_into_io_error(err: cbor4ii::serde::DecodeError<Infallible>) -> io::Error {
    match err {
        cbor4ii::serde::DecodeError::Core(DecodeError::Read(e)) => {
            io::Error::new(io::ErrorKind::Other, e)
        }
        cbor4ii::serde::DecodeError::Core(e @ DecodeError::Unsupported { .. }) => {
            io::Error::new(io::ErrorKind::Unsupported, e)
        }
        cbor4ii::serde::DecodeError::Core(e @ DecodeError::Eof { .. }) => {
            io::Error::new(io::ErrorKind::UnexpectedEof, e)
        }
        cbor4ii::serde::DecodeError::Core(e) => io::Error::new(io::ErrorKind::InvalidData, e),
        cbor4ii::serde::DecodeError::Custom(e) => {
            io::Error::new(io::ErrorKind::Other, e.to_string())
        }
    }
}

fn encode_into_io_error(err: cbor4ii::serde::EncodeError<TryReserveError>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}
