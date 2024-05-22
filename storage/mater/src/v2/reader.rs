use ipld_core::cid::Cid;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::index::read_index;
use crate::{
    v2::{index::Index, Characteristics, Header, PRAGMA},
    Error,
};

/// Low-level CARv2 reader.
pub struct Reader<R> {
    reader: R,
}

impl<R> Reader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader<R>
where
    R: AsyncRead + Unpin,
{
    pub async fn read_pragma(&mut self) -> Result<Vec<u8>, Error> {
        let mut pragma_buffer = vec![0; PRAGMA.len()];
        self.reader.read_exact(&mut pragma_buffer).await?;
        // NOTE(@jmg-duarte,20/05/2024): Should we validate the pragma here?
        debug_assert_eq!(pragma_buffer, PRAGMA);
        Ok(pragma_buffer)
    }

    /// Read the [`Header`].
    pub async fn read_header(&mut self) -> Result<Header, Error> {
        // Even though the standard doesn't explicitly state endianness, go-car does
        // https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/car.go#L51-L69
        let characteristics_bitfield = self.reader.read_u128_le().await?;
        // NOTE(@jmg-duarte,19/05/2024): unsure if we should fail on unknown bits, truncate them, or ignore
        let characteristics = Characteristics::from_bits_retain(characteristics_bitfield);

        let data_offset = self.reader.read_u64_le().await?;
        let data_size = self.reader.read_u64_le().await?;
        let index_offset = self.reader.read_u64_le().await?;

        Ok(Header {
            characteristics,
            data_offset,
            data_size,
            index_offset,
        })
    }

    pub async fn read_v1_header(&mut self) -> Result<crate::v1::Header, Error> {
        Ok(crate::v1::read_header(&mut self.reader).await?)
    }

    pub async fn read_block(&mut self) -> Result<(Cid, Vec<u8>), Error> {
        Ok(crate::v1::read_block(&mut self.reader).await?)
    }

    pub async fn read_index(&mut self) -> Result<Index, Error> {
        read_index(&mut self.reader).await
    }

    /// Get a mutable reference to the inner reader.
    pub fn get_inner_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

#[cfg(test)]
mod tests {

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::{fs::File, io::AsyncSeekExt};

    use crate::{
        multihash::generate_multihash,
        v2::{index::Index, reader::Reader, PRAGMA},
        Error,
    };

    #[tokio::test]
    async fn pragma() {
        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let pragma = reader.read_pragma().await.unwrap();
        assert_eq!(pragma, PRAGMA);
    }

    #[tokio::test]
    async fn header() {
        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let _ = reader.read_pragma().await.unwrap();
        let header = reader.read_header().await.unwrap();

        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 7661);
        assert_eq!(header.index_offset, 7712);
    }

    #[tokio::test]
    async fn inner_car() {
        // Read the original file to get the multihash
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&file_contents);
        let contents_cid = Cid::new_v1(0x55, contents_multihash);

        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let _ = reader.read_pragma().await.unwrap();
        let header = reader.read_header().await.unwrap();

        let inner = reader.get_inner_mut();
        inner
            .seek(std::io::SeekFrom::Start(header.data_offset))
            .await
            .unwrap();

        let v1_header = reader.read_v1_header().await.unwrap();
        assert_eq!(v1_header.version, 1);
        assert_eq!(v1_header.roots, vec![contents_cid]);

        loop {
            match reader.read_block().await {
                Ok((cid, _)) => println!("{:?}", cid),
                else_ => {
                    assert!(matches!(else_, Err(Error::IoError(_))));
                    break;
                }
            }
        }
    }

    #[tokio::test]
    async fn indexes() {
        // Read the original file to get the multihash
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&file_contents);

        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let _ = reader.read_pragma().await.unwrap();
        let header = reader.read_header().await.unwrap();

        let inner = reader.get_inner_mut();
        inner
            .seek(std::io::SeekFrom::Start(header.index_offset))
            .await
            .unwrap();

        let index = reader.read_index().await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));
        if let Index::MultihashIndexSorted(mh) = index {
            assert_eq!(mh.0.len(), 1);
            assert!(mh.0.contains_key(&0x12));
            let fst = &mh.0[&0x12].0;
            assert_eq!(fst.len(), 1);
            assert_eq!(fst[0].count, 1);
            assert_eq!(fst[0].width, 40);
            assert_eq!(fst[0].entries.len(), 1);
            assert_eq!(fst[0].entries[0].offset, 59);
            assert_eq!(fst[0].entries[0].digest, contents_multihash.digest());
        }
    }

    #[tokio::test]
    async fn full_file_lorem() {
        // Read the original file to get the multihash
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&file_contents);
        let contents_cid = Cid::new_v1(0x55, contents_multihash);

        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let pragma = reader.read_pragma().await.unwrap();
        assert_eq!(pragma, PRAGMA);

        let header = reader.read_header().await.unwrap();
        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 7661);
        assert_eq!(header.index_offset, 7712);

        let v1_header = reader.read_v1_header().await.unwrap();
        assert_eq!(v1_header.version, 1);
        assert_eq!(v1_header.roots, vec![contents_cid]);

        loop {
            match reader.read_block().await {
                Ok((cid, _)) => {
                    // Kinda hacky, but better than doing a seek later on
                    let position = reader.get_inner_mut().stream_position().await.unwrap();
                    let data_end = header.data_offset + header.data_size;
                    if position >= data_end {
                        break;
                    }
                    println!("{:?}", cid);
                }
                else_ => {
                    assert!(matches!(else_, Err(Error::IoError(_))));
                    break;
                }
            }
        }

        let index = reader.read_index().await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));
        if let Index::MultihashIndexSorted(mh) = index {
            assert_eq!(mh.0.len(), 1);
            assert!(mh.0.contains_key(&0x12));
            let fst = &mh.0[&0x12].0;
            assert_eq!(fst.len(), 1);
            assert_eq!(fst[0].count, 1);
            assert_eq!(fst[0].width, 40);
            assert_eq!(fst[0].entries.len(), 1);
            assert_eq!(fst[0].entries[0].offset, 59);
            assert_eq!(fst[0].entries[0].digest, contents_multihash.digest());
        }
    }

    #[tokio::test]
    async fn full_file_glenda() {
        let file = File::open("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();
        let mut reader = Reader::new(file);
        let pragma = reader.read_pragma().await.unwrap();
        assert_eq!(pragma, PRAGMA);

        let header = reader.read_header().await.unwrap();
        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 654402);
        assert_eq!(header.index_offset, 654453);

        let v1_header = reader.read_v1_header().await.unwrap();
        assert_eq!(v1_header.version, 1);
        assert_eq!(v1_header.roots.len(), 1);
        assert_eq!(
            v1_header.roots[0]
                .to_string_of_base(ipld_core::cid::multibase::Base::Base32Lower)
                .unwrap(),
            // Taken from `car inspect tests/fixtures/car_v2/spaceglenda.car`
            "bafybeiefli7iugocosgirzpny4t6yxw5zehy6khtao3d252pbf352xzx5q"
        );

        loop {
            // TODO(@jmg-duarte,22/05/2024): review this
            match reader.read_block().await {
                Ok((_, _)) => {
                    // Kinda hacky, but better than doing a seek later on
                    let position = reader.get_inner_mut().stream_position().await.unwrap();
                    let data_end = header.data_offset + header.data_size;
                    if position >= data_end {
                        break;
                    }
                }
                else_ => {
                    // With the length check above this branch should actually be unreachable
                    assert!(matches!(else_, Err(Error::IoError(_))));
                    break;
                }
            }
        }

        let index = reader.read_index().await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));
        if let Index::MultihashIndexSorted(mh) = index {
            assert_eq!(mh.0.len(), 1);
            assert!(mh.0.contains_key(&0x12));
            let fst = &mh.0[&0x12].0;
            assert_eq!(fst.len(), 1);
            assert_eq!(fst[0].count, 4);
            assert_eq!(fst[0].width, 40);
            assert_eq!(fst[0].entries.len(), 4);
        }
    }
}
