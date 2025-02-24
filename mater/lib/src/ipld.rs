use ipld_core::cid::{multihash::Multihash, CidGeneric, Error, Version};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{async_varint::read_varint, IDENTITY_CODE};

/// Ipld-related extensions.
pub trait IpldExt {
    /// Reads the bytes from a byte stream, returns the read [`CidGeneric`] & number of bytes read.
    fn read_cid<const S: usize>(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(CidGeneric<S>, usize), Error>>;

    /// Reads the bytes from a byte stream, returns the read [`Multihash`] & number of bytes read.
    fn read_multihash<const S: usize>(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(Multihash<S>, usize), Error>>;
}

impl<R> IpldExt for R
where
    R: AsyncRead + Unpin,
{
    /// Async implementation of
    /// <https://github.com/multiformats/rust-cid/blob/eb03f566e9bfb19bad79b2691dbcb2541627c0b3/src/cid.rs#L143C12-L143C22>
    async fn read_cid<const S: usize>(&mut self) -> Result<(CidGeneric<S>, usize), Error> {
        let (version, version_bytes_read): (u64, usize) = read_varint(self).await?;
        let (codec, codec_bytes_read): (u64, usize) = read_varint(self).await?;

        // CIDv0 has the fixed `0x12 0x20` prefix
        if [version, codec] == [0x12, 0x20] {
            const DIGEST_SIZE: usize = 32;
            let mut digest = [0u8; DIGEST_SIZE];
            self.read_exact(&mut digest).await?;
            let mh = Multihash::wrap(version, &digest).expect("Digest is always 32 bytes.");

            let bytes_read = version_bytes_read + codec_bytes_read + DIGEST_SIZE;
            return Ok((CidGeneric::new_v0(mh)?, bytes_read));
        }

        let version = Version::try_from(version)?;
        match version {
            Version::V0 => Err(Error::InvalidExplicitCidV0),
            Version::V1 => {
                let (mh, multihash_bytes_read) = self.read_multihash().await?;
                let bytes_read = version_bytes_read + codec_bytes_read + multihash_bytes_read;
                Ok((CidGeneric::new(version, codec, mh)?, bytes_read))
            }
        }
    }

    /// Async implementation of
    /// <https://github.com/multiformats/rust-multihash/blob/90a6c19ec71ced09469eec164a3586aafeddfbbd/src/multihash.rs#L271>
    async fn read_multihash<const S: usize>(&mut self) -> Result<(Multihash<S>, usize), Error> {
        let (code, code_bytes_read): (u64, usize) = read_varint(self).await?;
        let (size, size_bytes_read): (u64, usize) = read_varint(self).await?;

        if size > S as u64 || size > u8::MAX as u64 {
            return Err(Error::ParsingError);
        }

        let mut digest = [0; S];
        self.read_exact(&mut digest[..size as usize])
            .await
            .map_err(|_| Error::ParsingError)?;

        let multihash = Multihash::wrap(code, &digest)
            .map_err(|_| Error::ParsingError)?
            .truncate(size as u8);

        let bytes_read = code_bytes_read + size_bytes_read + size as usize;

        Ok((multihash, bytes_read))
    }
}

/// Extension trait for [`Cid`](ipld_core::cid::Cid)
pub trait CidExt {
    /// Returns Some(data) if the CID is an identity. If not, None is returned.
    fn get_identity_data(&self) -> Option<&[u8]>;
}

impl<const S: usize> CidExt for CidGeneric<S> {
    fn get_identity_data(&self) -> Option<&[u8]> {
        (self.hash().code() == IDENTITY_CODE).then(|| self.hash().digest())
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, str::FromStr};

    use ipld_core::cid::{multihash::Multihash, Cid};

    use crate::{ipld::IpldExt, multicodec::SHA_256_CODE, RAW_CODE};

    #[tokio::test]
    async fn cid_v0_read_bytes_async() {
        let mh = Multihash::<64>::wrap(SHA_256_CODE, &[0u8; 32]).unwrap();
        let original_cid = Cid::new_v0(mh).unwrap();

        let mut cid_bytes = Cursor::new(original_cid.to_bytes());
        let (cid, bytes_read) = cid_bytes.read_cid().await.unwrap();

        assert_eq!(original_cid, cid);
        // In case of CIDv0. Multihash has an explicit size and codec. Because
        // of that, they are not part of the format and we are not reading them.
        // 34 = cid_version(1) + cid_codec(1) + multihash_digest_size(32)
        assert_eq!(bytes_read, 34);
    }

    #[tokio::test]
    async fn cid_v1_read_bytes_async() {
        let original_cid =
            Cid::from_str("bafkreiczsrdrvoybcevpzqmblh3my5fu6ui3tgag3jm3hsxvvhaxhswpyu").unwrap();

        let mut cid_bytes = Cursor::new(original_cid.to_bytes());
        let (cid, bytes_read) = cid_bytes.read_cid().await.unwrap();

        assert_eq!(original_cid, cid);
        // 36 = cid_version(1) + cid_codec(1) + mh_code(1) + mh_size(1) + multihash_digest_size(32)
        assert_eq!(bytes_read, 36);
    }

    #[tokio::test]
    async fn multihash_read_async() {
        let original_mh = Multihash::<64>::wrap(RAW_CODE, b"Hello World!").unwrap();

        let mut mh_bytes = Cursor::new(original_mh.to_bytes());
        let (mh, bytes_read) = mh_bytes.read_multihash::<64>().await.unwrap();

        assert_eq!(original_mh, mh);
        // 10 = mh_code(1) + mh_size(1) + multihash_digest_size(12)
        assert_eq!(bytes_read, 14);
    }

    #[tokio::test]
    async fn multihash_read_async_digest_size_error() {
        let original_mh = Multihash::<64>::wrap(RAW_CODE, b"Hello World!").unwrap();

        let mut mh_bytes = Cursor::new(original_mh.to_bytes());
        assert!(mh_bytes.read_multihash::<5>().await.is_err());
    }
}
