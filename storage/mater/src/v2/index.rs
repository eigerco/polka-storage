use std::collections::BTreeMap;

use integer_encoding::{VarIntAsyncReader, VarIntAsyncWriter};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::Error;

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

impl IndexEntry {
    /// Construct a new [`IndexEntry`].
    pub fn new(digest: Vec<u8>, offset: u64) -> Self {
        Self { digest, offset }
    }
}

/// An index containing a single digest length.
#[derive(Debug, PartialEq, Eq)]
pub struct SingleWidthIndex {
    /// The hash digest and the respective offset length.
    pub width: u32,

    /// The number of index entries.
    /// It is serialized as the length of all entries in bytes
    /// (i.e. `self.count * self.width`).
    ///
    /// See `go-car`'s source code for more information:
    /// https://github.com/ipld/go-car/blob/45b81c1cc5117b3340dfdb025afeca90bfbe8d86/v2/index/indexsorted.go#L29
    pub count: u64,

    /// The index entries.
    pub entries: Vec<IndexEntry>,
}

impl SingleWidthIndex {
    /// Construct a new [`SingleWidthIndex`].
    ///
    /// Notes:
    /// * The `digest_width` should not account for the offset length.
    /// * This function sorts the `entries`.
    fn new(digest_width: u32, count: u64, mut entries: Vec<IndexEntry>) -> Self {
        entries.sort_by(|fst, snd| fst.digest.cmp(&snd.digest));
        Self {
            width: digest_width + 8, // digest_width + offset len
            count,
            entries,
        }
    }
}

impl From<IndexEntry> for SingleWidthIndex {
    fn from(value: IndexEntry) -> Self {
        SingleWidthIndex::new(value.digest.len() as u32, 1, vec![value])
    }
}

impl TryFrom<Vec<IndexEntry>> for SingleWidthIndex {
    type Error = Error;

    /// Performs the conversion, validating that all indexes have the same width.
    fn try_from(value: Vec<IndexEntry>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(Error::EmptyIndexError);
        }
        let width = value[0].digest.len();
        let count = value.len();
        for entry in &value[1..] {
            if entry.digest.len() != width {
                return Err(Error::NonMatchingDigestError {
                    expected: width,
                    received: entry.digest.len(),
                });
            }
        }
        Ok(Self::new(width as u32, count as u64, value))
    }
}

/// An index containing hash digests of multiple lengths.
///
/// To find a given index entry, first find the right index width,
/// and then find the hash to the data block.
///
/// For more details, read the [`Format 0x0400: IndexSorted`](https://ipld.io/specs/transport/car/carv2/#format-0x0400-indexsorted) section in the CARv2 specification.
#[derive(Debug, PartialEq, Eq)]
pub struct MultiWidthIndex(pub Vec<SingleWidthIndex>);

impl From<IndexEntry> for MultiWidthIndex {
    fn from(value: IndexEntry) -> Self {
        Self(vec![SingleWidthIndex::from(value)])
    }
}

impl From<SingleWidthIndex> for MultiWidthIndex {
    fn from(value: SingleWidthIndex) -> Self {
        Self(vec![value])
    }
}

impl From<Vec<SingleWidthIndex>> for MultiWidthIndex {
    fn from(value: Vec<SingleWidthIndex>) -> Self {
        Self(value)
    }
}

/// An index mapping Multihash codes to [`MultiWidthIndex`].
///
/// For more details, read the [`Format 0x0401: MultihashIndexSorted`](https://ipld.io/specs/transport/car/carv2/#format-0x0401-multihashindexsorted) section in the CARv2 specification.
#[derive(Debug, PartialEq, Eq)]
pub struct MultihashIndexSorted(
    // NOTE(@jmg-duarte,21/05/2024): maybe we should implement Deref where Deref::Target = BTreeMap<u64, MultiwidthIndex>?
    pub BTreeMap<u64, MultiWidthIndex>,
);

impl From<BTreeMap<u64, MultiWidthIndex>> for MultihashIndexSorted {
    fn from(value: BTreeMap<u64, MultiWidthIndex>) -> Self {
        Self(value)
    }
}

/// CARv2 index.
#[derive(Debug, PartialEq, Eq)]
pub enum Index {
    IndexSorted(MultiWidthIndex),
    MultihashIndexSorted(MultihashIndexSorted),
}

impl Index {
    pub fn multihash(index: BTreeMap<u64, MultiWidthIndex>) -> Self {
        Self::MultihashIndexSorted(index.into())
    }
}

pub(crate) async fn write_index<W>(mut writer: W, index: &Index) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    match index {
        Index::IndexSorted(index) => {
            writer.write_varint_async(INDEX_SORTED_CODE).await?;
            write_index_sorted(&mut writer, index).await?;
        }
        Index::MultihashIndexSorted(index) => {
            writer
                .write_varint_async(MULTIHASH_INDEX_SORTED_CODE)
                .await?;
            write_multihash_index_sorted(&mut writer, index).await?
        }
    }
    Ok(())
}

pub(crate) async fn write_multihash_index_sorted<W>(
    mut writer: W,
    index: &MultihashIndexSorted,
) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    writer.write_i32_le(index.0.len() as i32).await?;
    for (hash_code, index) in index.0.iter() {
        writer.write_u64_le(*hash_code).await?;
        write_index_sorted(&mut writer, index).await?;
    }
    Ok(())
}

pub(crate) async fn write_index_sorted<W>(
    mut writer: W,
    index: &MultiWidthIndex,
) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    writer.write_i32_le(index.0.len() as i32).await?;
    for idx in &index.0 {
        write_single_width_index(&mut writer, idx).await?;
    }
    Ok(())
}

pub(crate) async fn write_single_width_index<W>(
    mut writer: W,
    index: &SingleWidthIndex,
) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    writer.write_u32_le(index.width).await?;
    writer
        .write_u64_le(index.count * (index.width as u64))
        .await?;
    for entry in &index.entries {
        write_index_entry(&mut writer, entry).await?;
    }
    Ok(())
}

pub(crate) async fn write_index_entry<W>(mut writer: W, entry: &IndexEntry) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(&entry.digest).await?;
    writer.write_u64_le(entry.offset).await?;
    Ok(())
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
    use std::{
        collections::{BTreeMap, HashMap},
        io::Cursor,
    };

    use rand::{random, Rng};
    use sha2::{Digest, Sha256, Sha512};
    use tokio::{fs::File, io::AsyncSeekExt};

    use crate::{
        multihash::{generate_multihash, MultihashCode},
        v1::read_block,
        v2::index::{
            read_index, read_index_entry, read_index_sorted, read_multihash_index_sorted,
            read_single_width_index, write_index, write_index_entry, write_index_sorted,
            write_multihash_index_sorted, write_single_width_index, Index, IndexEntry,
            MultiWidthIndex, MultihashIndexSorted, SingleWidthIndex,
        },
    };

    fn generate_single_width_index<H>(count: u64) -> SingleWidthIndex
    where
        H: Digest,
    {
        let mut entries = vec![];
        let mut data = vec![0u8; <H as Digest>::output_size()];
        for idx in 0..count {
            data.fill_with(random);
            let digest = H::digest(&data).to_vec();
            entries.push(IndexEntry::new(digest, idx));
        }
        SingleWidthIndex::try_from(entries).unwrap()
    }

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

    #[tokio::test]
    async fn rountrip_index_entry() {
        let mut data = [0u8; 32];
        rand::thread_rng().fill(&mut data);
        let digest = Sha256::digest(data).to_vec();
        let entry = IndexEntry {
            digest: digest.clone(),
            offset: 42,
        };

        let mut buffer = vec![];
        write_index_entry(&mut buffer, &entry).await.unwrap();

        let mut reader = Cursor::new(buffer);
        let result = read_index_entry(&mut reader, 32).await.unwrap();
        assert_eq!(entry.digest, result.digest);
        assert_eq!(entry.offset, result.offset);
    }

    #[tokio::test]
    async fn roundtrip_single_width_index() {
        let single_width = generate_single_width_index::<Sha256>(5);

        let mut buffer = vec![];
        write_single_width_index(&mut buffer, &single_width)
            .await
            .unwrap();
        let mut reader = Cursor::new(buffer);
        let index = read_single_width_index(&mut reader).await.unwrap();
        assert_eq!(single_width, index);
    }

    #[tokio::test]
    async fn roundtrip_multiwidth_index() {
        let index = MultiWidthIndex(vec![
            generate_single_width_index::<Sha256>(5),
            generate_single_width_index::<Sha512>(5),
        ]);

        let mut buffer = vec![];
        write_index_sorted(&mut buffer, &index).await.unwrap();

        let mut reader = Cursor::new(buffer);
        let result = read_index_sorted(&mut reader).await.unwrap();

        assert_eq!(index, result);
    }

    #[tokio::test]
    async fn roundtrip_multihash_index() {
        let mut mapping = BTreeMap::new();
        mapping.insert(
            Sha256::CODE,
            generate_single_width_index::<Sha256>(5).into(),
        );
        mapping.insert(
            Sha512::CODE,
            generate_single_width_index::<Sha512>(5).into(),
        );
        let index = MultihashIndexSorted(mapping);

        let mut buffer = vec![];
        write_multihash_index_sorted(&mut buffer, &index)
            .await
            .unwrap();

        let mut reader = Cursor::new(buffer);
        let result = read_multihash_index_sorted(&mut reader).await.unwrap();

        assert_eq!(index, result);
    }

    #[tokio::test]
    async fn roundtrip_index_multihash() {
        let mut mapping = BTreeMap::new();
        mapping.insert(
            Sha256::CODE,
            generate_single_width_index::<Sha256>(5).into(),
        );
        mapping.insert(
            Sha512::CODE,
            generate_single_width_index::<Sha512>(5).into(),
        );
        let index = Index::MultihashIndexSorted(MultihashIndexSorted(mapping));

        let mut buffer = vec![];
        write_index(&mut buffer, &index).await.unwrap();

        let mut reader = Cursor::new(buffer);
        let result = read_index(&mut reader).await.unwrap();

        assert_eq!(index, result);
    }

    #[tokio::test]
    async fn roundtrip_index_sorted() {
        let index = Index::IndexSorted(MultiWidthIndex(vec![
            generate_single_width_index::<Sha256>(5),
            generate_single_width_index::<Sha512>(5),
        ]));

        let mut buffer = vec![];
        write_index(&mut buffer, &index).await.unwrap();

        let mut reader = Cursor::new(buffer);
        let result = read_index(&mut reader).await.unwrap();

        assert_eq!(index, result);
    }
}
