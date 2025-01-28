use ipld_core::cid::{multihash::Multihash, CidGeneric, Error, Version};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{async_varint::read_varint, IDENTITY_CODE};

pub trait MultihashExt {
    async fn read_async<R>(r: R) -> Result<(Self, usize), Error>
    where
        Self: Sized,
        R: AsyncRead + Unpin;
}

pub trait CidExt {
    async fn read_bytes_async<R>(r: R) -> Result<(Self, usize), Error>
    where
        Self: Sized,
        R: AsyncRead + Unpin;

    /// Returns Some(data) if the CID is an identity. If not, None is returned.
    fn get_identity_data(&self) -> Option<&[u8]>;
}

impl<const S: usize> CidExt for CidGeneric<S> {
    fn get_identity_data(&self) -> Option<&[u8]> {
        (self.hash().code() == IDENTITY_CODE).then(|| self.hash().digest())
    }

    /// Async implementation of
    /// https://github.com/multiformats/rust-cid/blob/eb03f566e9bfb19bad79b2691dbcb2541627c0b3/src/cid.rs#L143C12-L143C22
    async fn read_bytes_async<R>(mut r: R) -> Result<(Self, usize), Error>
    where
        R: AsyncRead + Unpin,
    {
        let (version, version_bytes_read): (u64, usize) = read_varint(&mut r).await?;
        let (codec, codec_bytes_read): (u64, usize) = read_varint(&mut r).await?;

        // CIDv0 has the fixed `0x12 0x20` prefix
        if [version, codec] == [0x12, 0x20] {
            const DIGEST_SIZE: usize = 32;
            let mut digest = [0u8; DIGEST_SIZE];
            r.read_exact(&mut digest).await?;
            let mh = Multihash::wrap(version, &digest).expect("Digest is always 32 bytes.");

            let bytes_read = version_bytes_read + codec_bytes_read + DIGEST_SIZE;
            return Ok((Self::new_v0(mh)?, bytes_read));
        }

        let version = Version::try_from(version)?;
        match version {
            Version::V0 => Err(Error::InvalidExplicitCidV0),
            Version::V1 => {
                let (mh, multihash_bytes_read) = Multihash::read_async(r).await?;
                let bytes_read = version_bytes_read + codec_bytes_read + multihash_bytes_read;
                Ok((Self::new(version, codec, mh)?, bytes_read))
            }
        }
    }
}

impl<const S: usize> MultihashExt for Multihash<S> {
    /// Async implementation of
    /// https://github.com/multiformats/rust-multihash/blob/90a6c19ec71ced09469eec164a3586aafeddfbbd/src/multihash.rs#L271
    async fn read_async<R>(mut r: R) -> Result<(Self, usize), Error>
    where
        Self: Sized,
        R: AsyncRead + Unpin,
    {
        let (code, code_bytes_read): (u64, usize) = read_varint(&mut r).await?;
        let (size, size_bytes_read): (u64, usize) = read_varint(&mut r).await?;

        if size > S as u64 || size > u8::MAX as u64 {
            return Err(Error::ParsingError);
        }

        let mut digest = [0; S];
        r.read_exact(&mut digest[..size as usize])
            .await
            .map_err(|_| Error::ParsingError)?;

        let multihash = Multihash::wrap(code, &digest)
            .map_err(|_| Error::ParsingError)?
            .truncate(size as u8);

        let bytes_read = code_bytes_read + size_bytes_read + size as usize;

        Ok((multihash, bytes_read))
    }
}
