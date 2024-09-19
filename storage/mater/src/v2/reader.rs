use ipld_core::cid::Cid;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};

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
    /// Constructs a new [`Reader`].
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader<R>
where
    R: AsyncRead + Unpin,
{
    /// Takes in a CID and checks that the contents in the reader matches this CID
    pub async fn verify_cid(&mut self, contents_cid: Cid) -> Result<bool, Error> {
        let _pragma = self.read_pragma().await?;
        let _header = self.read_header().await?;
        let v1_header = self.read_v1_header().await?;

        Ok(vec![contents_cid] == v1_header.roots)
    }

    /// Reads the contents of the CARv2 file and puts the contents into the supplied output file.
    pub async fn extract_content<W>(&mut self, output_file: &mut W) -> Result<(), Error>
    where
        W: AsyncWriteExt + Unpin,
        R: AsyncSeekExt,
    {
        self.read_pragma().await?;
        let header = self.read_header().await?;
        let _v1_header = self.read_v1_header().await?;
        let mut written = 0;

        while let Ok((_cid, contents)) = self.read_block().await {
            // CAR file contents is empty
            if contents.len() == 0 {
                break;
            }
            let position = self.get_inner_mut().stream_position().await?;
            let data_end = header.data_offset + header.data_size;
            // Add the `written != 0` clause for files that are less than a single block.
            if position >= data_end && written != 0 {
                break;
            }
            written += output_file.write(&contents).await?;
        }

        Ok(())
    }

    /// Read the CARv2 pragma.
    ///
    /// This function fails if the pragma does not match the one defined in the
    /// [specification](https://ipld.io/specs/transport/car/carv2/#pragma).
    pub async fn read_pragma(&mut self) -> Result<(), Error> {
        let mut pragma_buffer = vec![0; PRAGMA.len()];
        self.reader.read_exact(&mut pragma_buffer).await?;
        if pragma_buffer != PRAGMA {
            return Err(Error::InvalidPragmaError(pragma_buffer));
        }
        // Since we validate the pragma, there's no point in returning it.
        Ok(())
    }

    /// Read the [`Header`].
    ///
    /// This function fails if there are set bits that are not covered in the
    /// [characteristics specification](https://ipld.io/specs/transport/car/carv2/#characteristics).
    ///
    /// For more information check the [header specification](https://ipld.io/specs/transport/car/carv2/#header).
    pub async fn read_header(&mut self) -> Result<Header, Error> {
        // Even though the standard doesn't explicitly state endianness, go-car does
        // https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/car.go#L51-L69
        let characteristics_bitfield = self.reader.read_u128_le().await?;

        let characteristics = Characteristics::from_bits(characteristics_bitfield)
            .ok_or(Error::UnknownCharacteristicsError(characteristics_bitfield))?;

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

    /// Read the [`Header`].
    ///
    /// See [`crate::v1::Reader`] for more information.
    pub async fn read_v1_header(&mut self) -> Result<crate::v1::Header, Error> {
        crate::v1::read_header(&mut self.reader).await
    }

    /// Read a [`Cid`] and data block.
    ///
    /// See [`crate::v1::Reader`] for more information.
    pub async fn read_block(&mut self) -> Result<(Cid, Vec<u8>), Error> {
        crate::v1::read_block(&mut self.reader).await
    }

    /// Read an [`Index`].
    pub async fn read_index(&mut self) -> Result<Index, Error> {
        read_index(&mut self.reader).await
    }

    /// Get a mutable reference to the inner reader.
    ///
    /// This is useful to skip padding or perform other operations the
    /// [`Reader`] does not natively support.
    pub fn get_inner_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

/// Function verifies that a given CID matches the CID for the CAR file at the given path
pub async fn verify_cid<F: AsyncRead + Unpin>(file: F, contents_cid: Cid) -> Result<bool, Error> {
    let mut reader = Reader::new(BufReader::new(file));

    reader.verify_cid(contents_cid).await
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf, str::FromStr};

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::{fs::File, io::AsyncSeekExt};

    use crate::{
        multicodec::{generate_multihash, RAW_CODE, SHA_256_CODE},
        v2::{index::Index, reader::Reader},
        verify_cid, Error,
    };

    #[tokio::test]
    async fn test_verify_cid() {
        let path = PathBuf::from("tests/fixtures/car_v2/lorem.car");
        let file = File::open(&path).await.unwrap();
        // Taken from `car inspect tests/fixtures/car_v2/lorem.car`
        let contents_cid =
            Cid::from_str("bafkreidnmi5roys6exf5urwrplejyjvt3nrviryb4lafsjrggig357krlm").unwrap();
        assert_eq!(verify_cid(file, contents_cid).await.unwrap(), true)
    }

    #[tokio::test]
    async fn pragma() {
        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let pragma = reader.read_pragma().await;
        assert!(matches!(pragma, Ok(())));
    }

    #[tokio::test]
    async fn bad_pragma() {
        let mut bad_pragma = vec![0u8; 11];
        bad_pragma.fill_with(rand::random);
        let mut reader = Reader::new(Cursor::new(bad_pragma));
        let pragma = reader.read_pragma().await;
        assert!(matches!(pragma, Err(Error::InvalidPragmaError(_))));
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
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let contents_cid = Cid::new_v1(RAW_CODE, contents_multihash);

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
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);

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
            assert!(mh.0.contains_key(&SHA_256_CODE));
            let fst = &mh.0[&SHA_256_CODE].0;
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
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let contents_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        reader.read_pragma().await.unwrap();

        let header = reader.read_header().await.unwrap();
        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 7661);
        assert_eq!(header.index_offset, 7712);

        let v1_header = reader.read_v1_header().await.unwrap();
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
            assert!(mh.0.contains_key(&SHA_256_CODE));
            let fst = &mh.0[&SHA_256_CODE].0;
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
        reader.read_pragma().await.unwrap();

        let header = reader.read_header().await.unwrap();
        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 654402);
        assert_eq!(header.index_offset, 654453);

        let v1_header = reader.read_v1_header().await.unwrap();
        assert_eq!(v1_header.roots.len(), 1);
        assert_eq!(
            v1_header.roots[0]
                .to_string_of_base(ipld_core::cid::multibase::Base::Base32Lower)
                .unwrap(),
            // Taken from `car inspect tests/fixtures/car_v2/spaceglenda.car`
            "bafybeiefli7iugocosgirzpny4t6yxw5zehy6khtao3d252pbf352xzx5q"
        );

        loop {
            // NOTE(@jmg-duarte,22/05/2024): review this
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
            assert!(mh.0.contains_key(&SHA_256_CODE));
            let fst = &mh.0[&SHA_256_CODE].0;
            assert_eq!(fst.len(), 1);
            assert_eq!(fst[0].count, 4);
            assert_eq!(fst[0].width, 40);
            assert_eq!(fst[0].entries.len(), 4);
        }
    }
}
