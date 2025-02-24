use ipld_core::cid::Cid;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::index::read_index;
use crate::v1::CarReader as _;
use crate::{
    v1::{self},
    v2::{index::Index, Characteristics, Header, PRAGMA},
    Error,
};

/// CAR v2 specific reading functions.
pub trait CarReader {
    /// Read the CARv2 pragma.
    ///
    /// This function fails if the pragma does not match the one defined in the
    /// [specification](https://ipld.io/specs/transport/car/carv2/#pragma).
    fn read_pragma(&mut self) -> impl std::future::Future<Output = Result<(), Error>>;

    /// Read the [`Header`].
    ///
    /// This function fails if there are set bits that are not covered in the
    /// [characteristics specification](https://ipld.io/specs/transport/car/carv2/#characteristics).
    ///
    /// For more information check the [header specification](https://ipld.io/specs/transport/car/carv2/#header).
    fn read_v2_header(&mut self) -> impl std::future::Future<Output = Result<Header, Error>>;

    /// Read an [`Index`].
    fn read_index(&mut self) -> impl std::future::Future<Output = Result<Index, Error>>;
}

impl<R> CarReader for R
where
    R: AsyncRead + Unpin,
{
    async fn read_pragma(&mut self) -> Result<(), Error> {
        let mut pragma_buffer = vec![0; PRAGMA.len()];
        self.read_exact(&mut pragma_buffer).await?;
        if pragma_buffer != PRAGMA {
            return Err(Error::InvalidPragmaError(pragma_buffer));
        }
        // Since we validate the pragma, there's no point in returning it.
        Ok(())
    }

    async fn read_v2_header(&mut self) -> Result<Header, Error> {
        // Even though the standard doesn't explicitly state endianness, go-car does
        // https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/car.go#L51-L69
        let characteristics_bitfield = self.read_u128_le().await?;

        let characteristics = Characteristics::from_bits(characteristics_bitfield)
            .ok_or(Error::UnknownCharacteristicsError(characteristics_bitfield))?;

        let data_offset = self.read_u64_le().await?;
        let data_size = self.read_u64_le().await?;
        let index_offset = self.read_u64_le().await?;

        Ok(Header {
            characteristics,
            data_offset,
            data_size,
            index_offset,
        })
    }

    async fn read_index(&mut self) -> Result<Index, Error> {
        read_index(self).await
    }
}

/// Extensions to [`CarReader`].
pub trait CarReaderExt: CarReader + v1::CarReader {
    /// Checks if the contents of the reader is a CARv2 file.
    fn is_car_file(&mut self) -> impl std::future::Future<Output = Result<(), Error>>;

    /// Takes in a CID and checks that the contents in the reader matches this CID
    fn verify_cid(
        &mut self,
        contents_cid: Cid,
    ) -> impl std::future::Future<Output = Result<(), Error>>;
}

impl<R> CarReaderExt for R
where
    R: AsyncRead + Unpin,
{
    async fn is_car_file(&mut self) -> Result<(), Error> {
        let _pragma = self.read_pragma().await?;
        let _header = self.read_v2_header().await?;
        let _v1_header = self.read_v1_header().await?;
        Ok(())
    }

    async fn verify_cid(&mut self, contents_cid: Cid) -> Result<(), Error> {
        let _pragma = self.read_pragma().await?;
        let _header = self.read_v2_header().await?;
        let v1_header = self.read_v1_header().await?;

        if [contents_cid] != *v1_header.roots {
            return Err(Error::InvalidCid);
        }

        let mut cid_sum = Cid::default();
        // Loop thru all CIDs to get the last one, which is the sum.
        while let Ok((cid, _contents)) = self.read_block().await {
            cid_sum = cid
        }

        if cid_sum == contents_cid {
            Ok(())
        } else {
            Err(Error::InvalidCid)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf, str::FromStr};

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::{fs::File, io::AsyncSeekExt};

    use crate::{
        multicodec::{generate_multihash, RAW_CODE, SHA_256_CODE},
        v1,
        v2::{
            index::Index,
            reader::{CarReader, CarReaderExt},
        },
        Error,
    };

    #[tokio::test]
    async fn failure_verifying_cid() {
        let path = PathBuf::from("tests/fixtures/car_v2/spaceglenda.car");
        let mut file = File::open(&path).await.unwrap();

        let wrong_cid =
            Cid::from_str("bafkreidnmi5roys6exf5urwrplejyjvt3nrviryb4lafsjrggig357krlm").unwrap();
        let result = file.verify_cid(wrong_cid).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_cid_empty() {
        let path = PathBuf::from("tests/fixtures/car_v2/empty.car");
        let mut file = File::open(&path).await.unwrap();
        // Taken from `car inspect tests/fixtures/car_v2/spaceglenda.car`
        let contents_cid =
            Cid::from_str("bafybeib37argqu7zjibjqwiekvvjtv6lby76ruft4dkysyfqdhvk4o3gvy").unwrap();
        let result = file.verify_cid(contents_cid).await;
        assert!(matches!(result, Ok(())));
    }

    #[tokio::test]
    async fn test_verify_cid_spaceglenda() {
        let path = PathBuf::from("tests/fixtures/car_v2/spaceglenda.car");
        let mut file = File::open(&path).await.unwrap();
        // Taken from `car inspect tests/fixtures/car_v2/spaceglenda.car`
        let contents_cid =
            Cid::from_str("bafybeiefli7iugocosgirzpny4t6yxw5zehy6khtao3d252pbf352xzx5q").unwrap();
        let result = file.verify_cid(contents_cid).await;
        assert!(matches!(result, Ok(())));
    }

    #[tokio::test]
    async fn test_verify_cid_lorem() {
        let path = PathBuf::from("tests/fixtures/car_v2/lorem.car");
        let mut file = File::open(&path).await.unwrap();
        // Taken from `car inspect tests/fixtures/car_v2/lorem.car`
        let contents_cid =
            Cid::from_str("bafkreidnmi5roys6exf5urwrplejyjvt3nrviryb4lafsjrggig357krlm").unwrap();
        let result = file.verify_cid(contents_cid).await;
        assert!(matches!(result, Ok(())));
    }

    #[tokio::test]
    async fn pragma() {
        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let pragma = file.read_pragma().await;
        assert!(matches!(pragma, Ok(())));
    }

    #[tokio::test]
    async fn bad_pragma() {
        let mut bad_pragma = vec![0u8; 11];
        bad_pragma.fill_with(rand::random);
        let mut reader = Cursor::new(bad_pragma);
        let pragma = reader.read_pragma().await;
        assert!(matches!(pragma, Err(Error::InvalidPragmaError(_))));
    }

    #[tokio::test]
    async fn header() {
        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let _ = file.read_pragma().await.unwrap();
        let header = file.read_v2_header().await.unwrap();

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

        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let _ = file.read_pragma().await.unwrap();
        let header = file.read_v2_header().await.unwrap();

        file.seek(std::io::SeekFrom::Start(header.data_offset))
            .await
            .unwrap();

        let v1_header = v1::CarReader::read_v1_header(&mut file).await.unwrap();
        assert_eq!(v1_header.roots, vec![contents_cid]);

        loop {
            match v1::CarReader::read_block(&mut file).await {
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

        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let _ = file.read_pragma().await.unwrap();
        let header = file.read_v2_header().await.unwrap();

        file.seek(std::io::SeekFrom::Start(header.index_offset))
            .await
            .unwrap();

        let index = file.read_index().await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));
        if let Index::MultihashIndexSorted(mh) = index {
            assert_eq!(mh.len(), 1);
            assert!(mh.contains_key(&SHA_256_CODE));
            let fst = &mh[&SHA_256_CODE];
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

        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        file.read_pragma().await.unwrap();

        let header = file.read_v2_header().await.unwrap();
        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 7661);
        assert_eq!(header.index_offset, 7712);

        let v1_header = v1::CarReader::read_v1_header(&mut file).await.unwrap();
        assert_eq!(v1_header.roots, vec![contents_cid]);

        loop {
            match v1::CarReader::read_block(&mut file).await {
                Ok((cid, _)) => {
                    // Kinda hacky, but better than doing a seek later on
                    let position = file.stream_position().await.unwrap();
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

        let index = file.read_index().await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));
        if let Index::MultihashIndexSorted(mh) = index {
            assert_eq!(mh.len(), 1);
            assert!(mh.contains_key(&SHA_256_CODE));
            let fst = &mh[&SHA_256_CODE];
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
        let mut file = File::open("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();
        file.read_pragma().await.unwrap();

        let header = file.read_v2_header().await.unwrap();
        // `car inspect tests/fixtures/car_v2/lorem.car` to get the values
        assert_eq!(header.characteristics.bits(), 0);
        assert_eq!(header.data_offset, 51);
        assert_eq!(header.data_size, 654402);
        assert_eq!(header.index_offset, 654453);

        let v1_header = v1::CarReader::read_v1_header(&mut file).await.unwrap();
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
            match v1::CarReader::read_block(&mut file).await {
                Ok((_, _)) => {
                    // Kinda hacky, but better than doing a seek later on
                    let position = file.stream_position().await.unwrap();
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

        let index = file.read_index().await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));
        if let Index::MultihashIndexSorted(mh) = index {
            assert_eq!(mh.len(), 1);
            assert!(mh.contains_key(&SHA_256_CODE));
            let fst = &mh[&SHA_256_CODE];
            assert_eq!(fst.len(), 1);
            assert_eq!(fst[0].count, 4);
            assert_eq!(fst[0].width, 40);
            assert_eq!(fst[0].entries.len(), 4);
        }
    }
}
