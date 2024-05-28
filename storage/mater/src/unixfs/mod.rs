//! The original implementation of this module is located at
//! <https://github.com/n0-computer/beetle/blob/3e137cb2bc18e1d458c3f72d5e817b03d9537d5d/iroh-unixfs/src/balanced_tree.rs>.

mod unixfs_pb;

use std::{collections::VecDeque, thread::current, vec::Drain};

use async_stream::try_stream;
use bytes::Bytes;
use futures::{TryStream, TryStreamExt};
use ipld_core::{
    cid::{Cid, CidGeneric},
    codec::Codec,
};
use ipld_dagpb::{DagPbCodec, PbLink, PbNode};
use itertools::{IntoChunks, Itertools};
use quick_protobuf::MessageWrite;
use sha2::{Digest, Sha256};
use tokio::{fs::File, io::AsyncRead, runtime::TryCurrentError};
use tokio_stream::{Stream, StreamExt};
use tokio_util::io::ReaderStream;

use crate::{
    multicodec::{generate_multihash, DAG_PB_CODE, RAW_CODE},
    Error,
};

#[derive(Debug)]
pub(crate) struct LinkInfo {
    raw_data_length: u64,
    encoded_data_length: u64,
}

impl LinkInfo {
    fn new(raw_data_length: u64, encoded_data_length: u64) -> Self {
        Self {
            raw_data_length,
            encoded_data_length,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Block {
    pub cid: Cid,
    pub data: Bytes,
    pub links: Vec<Cid>,
}

impl Block {
    fn new(cid: Cid, data: Bytes, links: Vec<Cid>) -> Self {
        Self { cid, data, links }
    }
}

#[derive(Debug)]
enum TreeNode {
    Leaf(Bytes),
    Stem(Vec<(Cid, LinkInfo)>),
}

impl TreeNode {
    fn encode(self) -> Result<(Block, LinkInfo), Error> {
        match self {
            TreeNode::Leaf(bytes) => {
                let data_length = bytes.len() as u64;
                let multihash = generate_multihash::<Sha256, _>(&bytes);
                // Storing the block as RAW as go-car does
                // TODO(@jmg-duarte,27/05/2024): find the go-car link
                let cid = Cid::new_v1(RAW_CODE, multihash);
                let block = Block::new(cid, bytes, vec![]);
                // The data is raw, so the raw length == encoded length
                let link_info = LinkInfo::new(data_length, data_length);
                Ok((block, link_info))
            }
            TreeNode::Stem(links) => {
                // TODO(@jmg-duarte,27/05/2024): the last issue is somewhere around here
                // there are extra bytes and hashes being calculated on the wrong content
                // presumably the extra bytes

                let mut encoded_length: u64 =
                    links.iter().map(|(_, l)| l.encoded_data_length).sum();
                let blocksizes: Vec<_> = links.iter().map(|(_, l)| l.raw_data_length).collect();
                let filesize: u64 = blocksizes.iter().sum();
                let pb_links: Vec<_> = links
                    .into_iter()
                    .map(|(cid, link)| PbLink {
                        cid,
                        // Having an empty name makes it compliant with go-car
                        name: Some("".to_string()),
                        size: Some(link.encoded_data_length),
                    })
                    .collect();

                let pb_node_data = unixfs_pb::Data {
                    Type: unixfs_pb::mod_Data::DataType::File,
                    filesize: Some(filesize),
                    blocksizes,
                    ..Default::default()
                };
                let mut pb_node_data_bytes = vec![];
                let mut pb_node_data_writer = quick_protobuf::Writer::new(&mut pb_node_data_bytes);
                pb_node_data.write_message(&mut pb_node_data_writer)?;
                let pb_node_data_length = pb_node_data_bytes.len() as u64;

                let pb_node = PbNode {
                    links: pb_links,
                    data: Some(pb_node_data_bytes.into()),
                };

                let outer = DagPbCodec::encode_to_vec(&pb_node)?;
                let cid = Cid::new_v1(DAG_PB_CODE, generate_multihash::<Sha256, _>(&outer));
                encoded_length += outer.len() as u64;

                Ok((
                    Block::new(
                        cid,
                        outer.into(),
                        pb_node.links.iter().map(|link| link.cid).collect(),
                    ),
                    LinkInfo {
                        raw_data_length: pb_node_data_length,
                        encoded_data_length: encoded_length,
                    },
                ))
            }
        }
    }
}

/// This function takes a stream of chunks of bytes and returns a stream of [`Block`]s.
///
/// It works by accumulating `width` blocks and lazily creating stems.
/// The tree grows upwards and does not keep previously completed `width` blocks.
///
/// As a demonstration, consider a `width` of 2 and an `input` stream that will yield 7 blocks.
/// ```text
/// Input stream <- Block 1, Block 2, Block 3, Block 4, Block 5, Block 6, Block 7
/// ```
///
/// Each time a block is taken out of the stream, it is stored in the lower level of the tree,
/// but it is also yielded as output:
/// ```text
/// Input stream <- Block 2, Block 3, Block 4, Block 5, Block 6, Block 7
/// Tree: [
///     [Block 1]
/// ]
/// Output stream -> Block 1
/// ```
///
/// Once the first `width` blocks (in this case, 2) are taken from the stream:
/// * A new stem is added, linking back to the two blocks
/// ```text
/// Input stream <- | Block 3 | Block 4 | Block 5 | Block 6 | Block 7 |
/// Tree: [
///     [Block 1, Block 2],
///     [Stem (B1, B2)]
/// ]
/// ```
/// * The previous level to the stem is evicted
/// ```text
/// Input stream <- | Block 3 | Block 4 | Block 5 | Block 6 | Block 7 |
/// Tree: [
///     [],
///     [Stem 1 (B1, B2)]
/// ]
/// ```
/// * The new stem is yielded
/// ```text
/// Input stream <- Block 3, Block 4, Block 5, Block 6, Block 7
/// Tree: [
///     [],
///     [Stem 1 (B1, B2)]
/// ]
/// Output stream -> Stem (B1, B2)
/// ```
///
/// This process happens recursively, so when the stem level is full, like so:
/// ```text
/// Input stream <- Block 5, Block 6, Block 7
/// Tree: [
///     [],
///     [Stem 1 (B1, B2), Stem 2 (B3, B4)]
/// ]
/// ```
///
/// A new stem is built upwards:
/// ```text
/// Input stream <- Block 5, Block 6, Block 7
/// Tree: [
///     [],
///     [],
///     [Stem 3 (S1, S2)]
/// ]
/// Output stream -> Stem 3 (S1, S2)
/// ```
///
/// Once the stream is exhausted, we need to clean up any remaining state:
/// ```text
/// Input stream <-
/// Tree: [
///     [Block 7],
///     [Stem 4 (B5, B6)],
///     [Stem 3 (S1, S2)],
/// ]
/// ```
///
/// In this case, the yielded tree looks like:
/// ```text
///       S3
///     /    \
///   S1      S2     S4
///  /  \    /  \   /  \
/// B1  B2  B3  B4 B5  B6  B7
/// ```
///
/// We work bottom-up, removing the levels one by one, creating new stems from them and returning the stems:
/// ```text
/// Tree: [
///     [], # popped
///     [Stem 4 (B5, B6), Stem 5 (B7)],
///     [Stem 3 (S1, S2)]
/// ]
/// Output stream -> Stem 5 (B7)
/// ```
///
/// The previous tree now looks like:
/// ```text
///       S3
///     /    \
///   S1      S2     S4    S5
///  /  \    /  \   /  \   |
/// B1  B2  B3  B4 B5  B6  B7
/// ```
///
/// If we repeat the process again:
/// ```text
/// Tree: [
///     [Stem 4 (B5, B6), Stem 5 (B7)], # popped
///     [Stem 3 (S1, S2), Stem 6 (S4, S5)]
/// ]
/// Output stream -> Stem 6 (S4, S5)
/// ```
///
/// The tree becomes:
/// ```text
///       S3            S6
///     /    \         /  \
///   S1      S2     S4    S5
///  /  \    /  \   /  \   |
/// B1  B2  B3  B4 B5  B6  B7
/// ```
///
/// And finally, we build the last stem, yielding it:
/// ```text
/// Tree: [
///     [Stem 3 (S1, S2), Stem 6 (S4, S5)] # popped
/// ]
/// Output stream -> Stem 7 (S3, S6)
/// ```
///
/// Making the final tree:
/// ```text
///              S7
///         /          \
///       S3            S6
///     /    \         /  \
///   S1      S2     S4    S5
///  /  \    /  \   /  \   |
/// B1  B2  B3  B4 B5  B6  B7
/// ```
///
/// The original implementation is in
/// <https://github.com/n0-computer/beetle/blob/3e137cb2bc18e1d458c3f72d5e817b03d9537d5d/iroh-unixfs/src/balanced_tree.rs#L50-L151>.
pub(crate) fn stream_balanced_tree<I>(
    input: I,
    width: usize,
) -> impl Stream<Item = Result<Block, Error>>
where
    I: Stream<Item = std::io::Result<Bytes>> + Send,
{
    try_stream! {
        let mut tree: VecDeque<Vec<(Cid, LinkInfo)>> = VecDeque::new();
        tree.push_back(vec![]);

        let input = input
            .err_into::<Error>()
            // The TreeNode::Leaf(data).encode() just wraps it with a Cid marking the payload as Raw
            // we may be able move this responsibility to the caller for more efficient memory usage
            .map(|data| data.and_then(|data| TreeNode::Leaf(data).encode()))
            .err_into::<Error>();
        tokio::pin!(input);

        while let Some(data) = input.next().await {
            let (block, link_info) = data?;
            let tree_height = tree.len();

            // Check if the leaf node is full
            // i.e. we can build a new stem
            if tree[0].len() == width {
                // Go up the tree, as adding a new stem
                // may complete another level and so on
                for level in 0..tree_height {
                    // If a node is not full, stop there
                    // no more "stem-ing" to be done
                    if tree[level].len() < width {
                        break;
                    }

                    // If we're at the top of the tree, we're going to need another level.
                    if level == tree_height - 1 {
                        tree.push_back(Vec::with_capacity(width));
                    }

                    // Replace the previous level elements with a new empty vector
                    // while `tree[level].drain().collect<Vec<_>>` is much more readable
                    // it's most likely less performant (I didn't measure)
                    // due to the different nature of the approaches (batch vs iterator)
                    let links = std::mem::replace(&mut tree[level], Vec::with_capacity(width));
                    let (block, link_info) = TreeNode::Stem(links).encode()?;
                    let cid = block.cid; // Cid: Copy
                    yield block;

                    tree[level + 1].push((cid, link_info));
                }
                // Once we're done "trimming" the tree
                // it's good to receive new elements
            }

            // If the tree level is empty, we can push,
            // if the tree level was not empty, the `for` took care of it
            tree[0].push((block.cid, link_info));
            yield block;
        }

        // If `input` yielded a single block,
        // the tree has height 1 and the lower level has a single element
        if tree.len() == 1 && tree[0].len() == 1 {
            return;
        }

        // Once `input` is exhausted, we need to perform cleanup of any leftovers,
        // to do so, we start by popping levels from the front and building stems over them.
        while let Some(links) = tree.pop_front() {
            let (block, link_info) = TreeNode::Stem(links).encode()?; // TODO
            let cid = block.cid;
            yield block;

            // If there's still a level in the front, it means the stem we just built will have a parent
            // we push the stem into the front level so we can build the parent on the next iteration
            if let Some(front) = tree.front_mut() {
                front.push((cid, link_info));
            }
            // Once there's nothing else in the front, that means we just yielded the root
            // and the current `while` will stop in the next iteration
        }
    }
}
