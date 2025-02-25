use ipld_core::{cid::Cid, codec::Codec};
use serde_ipld_dagcbor::codec::DagCborCodec;
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub use crate::v1::Header;
use crate::{async_varint::write_varint, Error};

/// Write [`crate::v1::Header`] to the provider writer.
pub(crate) async fn write_header<W>(writer: &mut W, header: &Header) -> Result<usize, Error>
where
    W: AsyncWrite + Unpin,
{
    let encoded_header = DagCborCodec::encode_to_vec(header)?;
    let varint_len = write_varint(writer, encoded_header.len()).await?;
    writer.write_all(&encoded_header).await?;
    Ok(varint_len + encoded_header.len())
}

/// Write a [`Cid`] and data block to the given writer.
///
/// This is a low-level function to be used in the implementation of CAR writers.
pub(crate) async fn write_block<W, Block>(
    writer: &mut W,
    cid: &Cid,
    block: Block,
) -> Result<usize, Error>
where
    W: AsyncWrite + Unpin,
    Block: AsRef<[u8]>,
{
    let data = block.as_ref();
    let len = cid.encoded_len() + data.len();

    let varint_len = write_varint(writer, len).await?;
    // This allocation can probably be spared
    writer.write_all(&cid.to_bytes()).await?;
    writer.write_all(block.as_ref()).await?;
    Ok(varint_len + cid.encoded_len() + block.as_ref().len())
}

/// Low-level, writing functions for the CAR format.
pub trait CarWriter {
    /// Write a [`crate::v1::Header`].
    fn write_header(
        &mut self,
        header: &Header,
    ) -> impl std::future::Future<Output = Result<usize, Error>>;

    /// Write a [`Cid`] and the respective data block.
    fn write_block<D>(
        &mut self,
        cid: &Cid,
        data: &D,
    ) -> impl std::future::Future<Output = Result<usize, Error>>
    where
        D: AsRef<[u8]>;
}

impl<W> CarWriter for W
where
    W: AsyncWrite + Unpin,
{
    async fn write_header(&mut self, header: &Header) -> Result<usize, Error> {
        write_header(self, header).await
    }

    async fn write_block<D>(&mut self, cid: &Cid, data: &D) -> Result<usize, Error>
    where
        D: AsRef<[u8]>,
    {
        write_block(self, cid, data).await
    }
}

#[cfg(test)]
mod tests {
    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::io::{AsyncWriteExt, BufWriter};

    use super::CarWriter;
    use crate::{
        multicodec::{generate_multihash, RAW_CODE},
        v1::Header,
    };

    #[tokio::test]
    async fn header_writer() {
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let buffer = Vec::new();
        let mut writer = BufWriter::new(buffer);
        writer
            .write_header(&Header::new(vec![root_cid]))
            .await
            .unwrap();
        writer.flush().await.unwrap();

        let expected_header = tokio::fs::read("tests/fixtures/car_v1/lorem_header.car")
            .await
            .unwrap();

        assert_eq!(&expected_header, writer.get_ref());
    }

    #[tokio::test]
    async fn full_writer() {
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let buffer = Vec::new();
        let mut writer = BufWriter::new(buffer);
        writer
            .write_header(&Header::new(vec![root_cid]))
            .await
            .unwrap();
        // There's only one block
        writer.write_block(&root_cid, &file_contents).await.unwrap();
        writer.flush().await.unwrap();

        let expected_header = tokio::fs::read("tests/fixtures/car_v1/lorem.car")
            .await
            .unwrap();
        assert_eq!(&expected_header, writer.get_ref());
    }
}
