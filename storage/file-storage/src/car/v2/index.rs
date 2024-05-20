use std::collections::BTreeMap;

use integer_encoding::VarIntAsyncReader;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::car::v2::Error;

pub const INDEX_SORTED_CODE: u64 = 0x0400;
pub const MULTIHASH_INDEX_SORTED_CODE: u64 = 0x0401;

// Basically, everything that does not have explicit endianness
// is little-endian, as made evident by the go-car source code
// https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/index/mhindexsorted.go#L45-L53

/// A index entry for a data block inside the CARv1.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexEntry {
    /// Hash digest of the data.
    pub digest: Vec<u8>,

    /// Offset to the first byte of the varint that prefix the CID:Bytes pair within the CARv1 payload.
    ///
    /// See the [data section in the CARv1 specification](https://ipld.io/specs/transport/car/carv1/#data)
    /// for details on block encoding.
    pub offset: u64,
}

/// An index containing a single digest length.
#[derive(Debug)]
pub struct SingleWidthIndex {
    /// The hash digest and the respective offset length.
    pub width: u32,

    /// The number of index entries.
    pub count: u64,

    /// The index entries.
    pub entries: Vec<IndexEntry>,
}

/// An index containing hash digests of multiple lengths.
///
/// To find a given index entry, first find the right index width,
/// and then find the hash to the data block.
///
/// For more details, read the [`Format 0x0400: IndexSorted`](https://ipld.io/specs/transport/car/carv2/#format-0x0400-indexsorted) section in the CARv2 specification.
#[derive(Debug)]
pub struct MultiWidthIndex(pub Vec<SingleWidthIndex>);

/// An index mapping Multihash codes to [`MultiWidthIndex`].
///
/// For more details, read the [`Format 0x0401: MultihashIndexSorted`](https://ipld.io/specs/transport/car/carv2/#format-0x0401-multihashindexsorted) section in the CARv2 specification.
#[derive(Debug)]
pub struct MultihashIndexSorted(pub BTreeMap<u64, MultiWidthIndex>);

/// CARv2 index.
#[derive(Debug)]
pub enum Index {
    IndexSorted(MultiWidthIndex),
    MultihashIndexSorted(MultihashIndexSorted),
}

pub(crate) async fn read_index<R>(mut reader: R) -> Result<Index, Error>
where
    R: AsyncRead + Unpin,
{
    let index_type: u64 = reader.read_varint_async().await?;
    return match index_type {
        INDEX_SORTED_CODE => Ok(Index::IndexSorted(read_index_sorted(&mut reader).await?)),
        MULTIHASH_INDEX_SORTED_CODE => Ok(Index::MultihashIndexSorted(
            read_multihash_index_sorted(&mut reader).await?,
        )),
        other => Err(Error::UnknownIndexError(other)),
    };
}

pub(crate) async fn read_multihash_index_sorted<R>(
    mut reader: R,
) -> Result<MultihashIndexSorted, Error>
where
    R: AsyncRead + Unpin,
{
    let n_indexes = reader.read_i32_le().await?;
    let mut indexes = BTreeMap::new();
    for _ in 0..n_indexes {
        let multihash_code = reader.read_u64_le().await?;
        let index = read_index_sorted(&mut reader).await?;
        indexes.insert(multihash_code, index);
    }
    Ok(MultihashIndexSorted(indexes))
}

pub(crate) async fn read_index_sorted<R>(mut reader: R) -> Result<MultiWidthIndex, Error>
where
    R: AsyncRead + Unpin,
{
    let n_buckets = reader.read_i32_le().await?;
    let mut buckets = Vec::with_capacity(n_buckets as usize);
    for _ in 0..n_buckets {
        let index = read_single_width_index(&mut reader).await?;
        buckets.push(index);
    }
    Ok(MultiWidthIndex(buckets))
}

pub(crate) async fn read_single_width_index<R>(mut reader: R) -> Result<SingleWidthIndex, Error>
where
    R: AsyncRead + Unpin,
{
    let width = reader.read_u32_le().await?;
    // Because someone decided that "total number of hash digests" means their length in bytes...
    // https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/index/indexsorted.go#L29
    let count = reader.read_u64_le().await? / (width as u64);
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        // The offset is always 8 bytes
        // https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/index/indexsorted.go#L176
        let entry = read_index_entry(&mut reader, width - 8).await?;
        entries.push(entry);
    }

    // Sorting by the digest only because it should be enough (famous last words)
    // > ... and finally within those buckets ordered by a simple byte-wise sorting.
    // — https://ipld.io/specs/transport/car/carv2/#format-0x0401-multihashindexsorted
    entries.sort_by(|fst, snd| fst.digest.cmp(&snd.digest));

    Ok(SingleWidthIndex {
        width,
        count,
        entries,
    })
}

pub(crate) async fn read_index_entry<R>(mut reader: R, length: u32) -> Result<IndexEntry, Error>
where
    R: AsyncRead + Unpin,
{
    let mut digest = vec![0; length as usize];
    reader.read_exact(&mut digest).await?;
    let offset = reader.read_u64_le().await?;
    Ok(IndexEntry { digest, offset })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        car::v1::read_block,
        car::v2::index::read_index,
        car::{generate_multihash, MultihashCode},
    };

    use super::{read_multihash_index_sorted, Index};
    use sha2::{Digest, Sha256};
    use tokio::{fs::File, io::AsyncSeekExt};

    #[tokio::test]
    async fn multihash_index_sorted_lorem() {
        let contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let digest = Sha256::digest(&contents);

        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        // We're skipping 2 bytes from the actual offset because we're not decoding the index type
        file.seek(std::io::SeekFrom::Start(7714)).await.unwrap();
        let index = read_multihash_index_sorted(file).await.unwrap();
        assert_eq!(index.0.len(), 1);
        assert!(index.0.contains_key(&Sha256::CODE));

        let multi_width_index = &index.0[&Sha256::CODE];
        assert_eq!(multi_width_index.0.len(), 1);

        let single_width_index = &multi_width_index.0[0];
        assert_eq!(single_width_index.width, 40);
        assert_eq!(single_width_index.count, 1);
        assert_eq!(single_width_index.entries.len(), 1);

        let entry = &single_width_index.entries[0];
        // Data offset: 51 & Hash length: 8
        assert_eq!(entry.offset, 51 + 8);
        assert_eq!(entry.digest, *digest);
    }

    /// `tests/fixtures/original/spaceglenda.jpg` generates a CARv2 file
    /// with multiple blocks, but not an insane amount, perfect for testing.
    #[tokio::test]
    async fn multihash_index_sorted_spaceglenda() {
        let mut file = File::open("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();
        // We're skipping 2 bytes from the actual offset because we're not decoding the index type
        file.seek(std::io::SeekFrom::Start(654455)).await.unwrap();
        let index = read_multihash_index_sorted(&mut file).await.unwrap();
        assert_eq!(index.0.len(), 1);
        assert!(index.0.contains_key(&Sha256::CODE));

        let multi_width_index = &index.0[&Sha256::CODE];
        assert_eq!(multi_width_index.0.len(), 1);

        let single_width_index = &multi_width_index.0[0];
        assert_eq!(single_width_index.width, 40);
        assert_eq!(single_width_index.count, 4);
        assert_eq!(single_width_index.entries.len(), 4);

        let mut codec_frequencies = HashMap::new();
        for entry in &single_width_index.entries {
            file.seek(std::io::SeekFrom::Start(
                51 + // Cheating a bit using the start data offset
                entry.offset,
            ))
            .await
            .unwrap();

            let (cid, block) = read_block(&mut file).await.unwrap();
            assert_eq!(cid.hash().code(), 0x12);

            // Sorting at this level is made byte-wise, so there's no short way
            // to compare the expected codecs...
            assert!(
                cid.codec() == 0x70 || // DAG-PB
                cid.codec() == 0x55 // RAW
            );
            // instead we build a frequency table and check against that later!
            if let Some(frequency) = codec_frequencies.get_mut(&cid.codec()) {
                *frequency += 1;
            } else {
                codec_frequencies.insert(cid.codec(), 1);
            }

            let multihash = generate_multihash::<Sha256>(&block);
            assert_eq!(cid.hash(), &multihash);
        }

        assert!(matches!(codec_frequencies.get(&0x70), Some(1)));
        assert!(matches!(codec_frequencies.get(&0x55), Some(3)));
    }

    #[tokio::test]
    async fn multihash_index_sorted_from_read_index() {
        let contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let digest = Sha256::digest(&contents);

        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();

        file.seek(std::io::SeekFrom::Start(7712)).await.unwrap();
        let index = read_index(file).await.unwrap();
        assert!(matches!(index, Index::MultihashIndexSorted(_)));

        if let Index::MultihashIndexSorted(index) = index {
            assert_eq!(index.0.len(), 1);
            assert!(index.0.contains_key(&Sha256::CODE));

            let multi_width_index = &index.0[&Sha256::CODE];
            assert_eq!(multi_width_index.0.len(), 1);

            let single_width_index = &multi_width_index.0[0];
            assert_eq!(single_width_index.width, 40);
            assert_eq!(single_width_index.count, 1);
            assert_eq!(single_width_index.entries.len(), 1);

            let entry = &single_width_index.entries[0];
            // Data offset: 51 & Hash length: 8
            assert_eq!(entry.offset, 51 + 8);
            assert_eq!(entry.digest, *digest);
        }
    }
}
