/// Utility functions for the mater crate. The contents were mostly borrowed
/// from the https://github.com/dermesser/integer-encoding-rs.
///
/// The original issue why we needed to borrow the implantation of the reader
/// and writer is
/// https://github.com/dermesser/integer-encoding-rs/blob/4f57046ae90b6b923ff235a91f0729d3cf868d72/src/writer.rs#L20.
/// This specifies the Send bound as optional. The side effect of this choice is
/// that all futures using the writer or reader are non Send and there is no way
/// to make them Send.
///
/// The second crate researched was
/// https://github.com/paritytech/unsigned-varint/tree/master. Issue with that
/// crate is that it only implements AsyncRead and AsyncWrite from the futures
/// crate and not tokio. For the future reference we could probably used
/// `unsigned-varint` with the tokio and use
/// https://docs.rs/tokio-util/latest/tokio_util/compat/trait.FuturesAsyncReadCompatExt.html
/// as the compatibility layer.
use std::{io, mem::size_of};

use integer_encoding::VarInt;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Write a VarInt integer to an asynchronous writer.
///
/// Borrowed from:
/// https://github.com/dermesser/integer-encoding-rs/blob/4f57046ae90b6b923ff235a91f0729d3cf868d72/src/writer.rs#L29
pub(crate) async fn write_varint<W, VI>(writer: &mut W, n: VI) -> Result<usize, io::Error>
where
    W: AsyncWrite + Unpin,
    VI: VarInt,
{
    let mut buf = [0 as u8; 10];
    let b = n.encode_var(&mut buf);
    writer.write_all(&buf[0..b]).await?;
    Ok(b)
}

/// Returns either the decoded integer, or an error.
///
/// In general, this always reads a whole varint. If the encoded varint's value
/// is bigger than the valid value range of `VI`, then the value is truncated.
///
/// On EOF, an io::Error with io::ErrorKind::UnexpectedEof is returned.
///
/// Borrowed from:
/// https://github.com/dermesser/integer-encoding-rs/blob/4f57046ae90b6b923ff235a91f0729d3cf868d72/src/reader.rs#L70
pub(crate) async fn read_varint<R, VI>(reader: &mut R) -> Result<VI, io::Error>
where
    R: AsyncRead + Unpin,
    VI: VarInt,
{
    let mut buf = [0_u8; 1];
    let mut p = VarIntProcessor::new::<VI>();

    while !p.finished() {
        let read = reader.read(&mut buf).await?;

        // EOF
        if read == 0 && p.i == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Reached EOF"));
        }
        if read == 0 {
            break;
        }

        p.push(buf[0])?;
    }

    p.decode()
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Reached EOF"))
}

/// Most-significant byte, == 0x80
const MSB: u8 = 0b1000_0000;

/// VarIntProcessor encapsulates the logic for decoding a VarInt byte-by-byte.
///
/// Borrowed from
/// https://github.com/dermesser/integer-encoding-rs/blob/4f57046ae90b6b923ff235a91f0729d3cf868d72/src/reader.rs#L35
#[derive(Default)]
struct VarIntProcessor {
    buf: [u8; 10],
    maxsize: usize,
    i: usize,
}

impl VarIntProcessor {
    fn new<VI: VarIntMaxSize>() -> VarIntProcessor {
        VarIntProcessor {
            maxsize: VI::varint_max_size(),
            ..VarIntProcessor::default()
        }
    }
    fn push(&mut self, b: u8) -> Result<(), io::Error> {
        if self.i >= self.maxsize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unterminated varint",
            ));
        }
        self.buf[self.i] = b;
        self.i += 1;
        Ok(())
    }
    fn finished(&self) -> bool {
        self.i > 0 && (self.buf[self.i - 1] & MSB == 0)
    }
    fn decode<VI: VarInt>(&self) -> Option<VI> {
        Some(VI::decode_var(&self.buf[0..self.i])?.0)
    }
}

/// Borrowed from
/// https://github.com/dermesser/integer-encoding-rs/blob/4f57046ae90b6b923ff235a91f0729d3cf868d72/src/varint.rs#L69
pub(crate) trait VarIntMaxSize {
    fn varint_max_size() -> usize;
}

impl<VI: VarInt> VarIntMaxSize for VI {
    fn varint_max_size() -> usize {
        (size_of::<VI>() * 8 + 7) / 7
    }
}
