//! The original implementation of this module is located at
//! <https://github.com/n0-computer/beetle/blob/3e137cb2bc18e1d458c3f72d5e817b03d9537d5d/iroh-unixfs/src/balanced_tree.rs>.

mod unixfs_pb;

use std::collections::VecDeque;

use async_stream::try_stream;
use bytes::Bytes;
use futures::TryStreamExt;
use ipld_core::{cid::Cid, codec::Codec};
use ipld_dagpb::{DagPbCodec, PbLink, PbNode};
use quick_protobuf::MessageWrite;
use sha2::Sha256;
use tokio_stream::{Stream, StreamExt};

use crate::{
    multicodec::{generate_multihash, DAG_PB_CODE, RAW_CODE},
    Error,
};

#[derive(Debug, Clone, Copy)]
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
enum TreeNode {
    Leaf(Bytes),
    Stem(Vec<(Cid, LinkInfo)>),
}

impl TreeNode {
    fn encode(self) -> Result<((Cid, Bytes), LinkInfo), Error> {
        match self {
            TreeNode::Leaf(bytes) => {
                let data_length = bytes.len() as u64;
                let multihash = generate_multihash::<Sha256, _>(&bytes);
                // Storing the block as RAW as go-car does
                // https://github.com/ipfs/go-unixfsnode/blob/c41f115d06cff90e0cbc634da5073b4c1447af09/data/builder/file.go#L54-L63
                let cid = Cid::new_v1(RAW_CODE, multihash);
                let block = (cid, bytes);
                // The data is raw, so the raw length == encoded length
                let link_info = LinkInfo::new(data_length, data_length);
                Ok((block, link_info))
            }
            TreeNode::Stem(links) => {
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
                    // NOTE(@jmg-duarte,28/05/2024): In the original implementation
                    // they have a `Block` structure that contains the child links,
                    // we're not currently using them and as such I didn't include them
                    (cid, outer.into()),
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
) -> impl Stream<Item = Result<(Cid, Bytes), Error>>
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
            let (block @ (cid, _), link_info) = data?;
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
                    let (block @ (cid, _), link_info) = TreeNode::Stem(links).encode()?;
                    yield block;

                    tree[level + 1].push((cid, link_info));
                }
                // Once we're done "trimming" the tree
                // it's good to receive new elements
            }

            // If the tree level is empty, we can push,
            // if the tree level was not empty, the `for` took care of it
            tree[0].push((cid, link_info));
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
            let (block @ (cid, _), link_info) = TreeNode::Stem(links).encode()?;
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

#[cfg(test)]
mod tests {
    //! Tests were taken from [beetle][beetle] too, I did modify them to suit our needs.
    //! In certain places, I made them check for byte equality as its way simpler
    //! and there's enough tests around the repo to ensure that if the underlying
    //! bytes are equal, the expected block sizes are as well.
    //!
    //! We also didn't write our own chunker, relying on [`tokio_util::io::ReadStream`] instead.
    //!
    //! [beetle]: https://github.com/n0-computer/beetle/blob/3e137cb2bc18e1d458c3f72d5e817b03d9537d5d/iroh-unixfs/src/balanced_tree.rs#L234-L507

    use bytes::BytesMut;
    use futures::StreamExt;

    use super::*;

    fn test_chunk_stream(num_chunks: usize) -> impl Stream<Item = std::io::Result<Bytes>> {
        futures::stream::iter((0..num_chunks).map(|n| Ok(n.to_be_bytes().to_vec().into())))
    }

    async fn build_expect_tree(num_chunks: usize, degree: usize) -> Vec<Vec<(Cid, Bytes)>> {
        let chunks = test_chunk_stream(num_chunks);
        tokio::pin!(chunks);
        let mut tree = vec![vec![]];
        let mut links = vec![vec![]];

        if num_chunks / degree == 0 {
            let chunk = chunks.next().await.unwrap().unwrap();
            let leaf = TreeNode::Leaf(chunk);
            let (block, _) = leaf.encode().unwrap();
            tree[0].push(block);
            return tree;
        }

        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.unwrap();
            let leaf = TreeNode::Leaf(chunk);
            let (block @ (cid, _), link_info) = leaf.encode().unwrap();
            links[0].push((cid, link_info));
            tree[0].push(block);
        }

        while tree.last().unwrap().len() > 1 {
            let prev_layer = links.last().unwrap();
            let count = prev_layer.len() / degree;
            let mut tree_layer = Vec::with_capacity(count);
            let mut links_layer = Vec::with_capacity(count);
            for links in prev_layer.chunks(degree) {
                let stem = TreeNode::Stem(links.to_vec());
                let (block @ (cid, _), link_info) = stem.encode().unwrap();
                links_layer.push((cid, link_info));
                tree_layer.push(block);
            }
            tree.push(tree_layer);
            links.push(links_layer);
        }
        tree
    }

    async fn build_expect_vec_from_tree(
        tree: Vec<Vec<(Cid, Bytes)>>,
        num_chunks: usize,
        degree: usize,
    ) -> Vec<(Cid, Bytes)> {
        let mut out = vec![];

        if num_chunks == 1 {
            out.push(tree[0][0].clone());
            return out;
        }

        let mut counts = vec![0; tree.len()];

        for leaf in tree[0].iter() {
            out.push(leaf.clone());
            counts[0] += 1;
            let mut push = counts[0] % degree == 0;
            for (num_layer, count) in counts.iter_mut().enumerate() {
                if num_layer == 0 {
                    continue;
                }
                if !push {
                    break;
                }
                out.push(tree[num_layer][*count].clone());
                *count += 1;
                if *count % degree != 0 {
                    push = false;
                }
            }
        }

        for (num_layer, count) in counts.into_iter().enumerate() {
            if num_layer == 0 {
                continue;
            }
            let layer = tree[num_layer].clone();
            for node in layer.into_iter().skip(count) {
                out.push(node);
            }
        }

        out
    }

    async fn build_expect(num_chunks: usize, degree: usize) -> Vec<(Cid, Bytes)> {
        let tree = build_expect_tree(num_chunks, degree).await;
        println!("{tree:?}");
        build_expect_vec_from_tree(tree, num_chunks, degree).await
    }

    fn make_leaf(data: usize) -> ((Cid, Bytes), LinkInfo) {
        TreeNode::Leaf(BytesMut::from(&data.to_be_bytes()[..]).freeze())
            .encode()
            .unwrap()
    }

    fn make_stem(links: Vec<(Cid, LinkInfo)>) -> ((Cid, Bytes), LinkInfo) {
        TreeNode::Stem(links).encode().unwrap()
    }

    #[tokio::test]
    async fn test_build_expect() {
        // manually build tree made of 7 chunks (11 total nodes)
        let (leaf_0, len_0) = make_leaf(0);
        let (leaf_1, len_1) = make_leaf(1);
        let (leaf_2, len_2) = make_leaf(2);
        let (stem_0, stem_len_0) = make_stem(vec![
            (leaf_0.0, len_0),
            (leaf_1.0, len_1),
            (leaf_2.0, len_2),
        ]);
        let (leaf_3, len_3) = make_leaf(3);
        let (leaf_4, len_4) = make_leaf(4);
        let (leaf_5, len_5) = make_leaf(5);
        let (stem_1, stem_len_1) = make_stem(vec![
            (leaf_3.0, len_3),
            (leaf_4.0, len_4),
            (leaf_5.0, len_5),
        ]);
        let (leaf_6, len_6) = make_leaf(6);
        let (stem_2, stem_len_2) = make_stem(vec![(leaf_6.0, len_6)]);
        let (root, _root_len) = make_stem(vec![
            (stem_0.0, stem_len_0),
            (stem_1.0, stem_len_1),
            (stem_2.0, stem_len_2),
        ]);

        let expect_tree = vec![
            vec![
                leaf_0.clone(),
                leaf_1.clone(),
                leaf_2.clone(),
                leaf_3.clone(),
                leaf_4.clone(),
                leaf_5.clone(),
                leaf_6.clone(),
            ],
            vec![stem_0.clone(), stem_1.clone(), stem_2.clone()],
            vec![root.clone()],
        ];
        let got_tree = build_expect_tree(7, 3).await;
        assert_eq!(expect_tree, got_tree);

        let expect_vec = vec![
            leaf_0, leaf_1, leaf_2, stem_0, leaf_3, leaf_4, leaf_5, stem_1, leaf_6, stem_2, root,
        ];
        let got_vec = build_expect_vec_from_tree(got_tree, 7, 3).await;
        assert_eq!(expect_vec, got_vec);
    }

    async fn ensure_equal(
        expect: Vec<(Cid, Bytes)>,
        got: impl Stream<Item = Result<(Cid, Bytes), Error>>,
    ) {
        let mut i = 0;
        tokio::pin!(got);
        while let Some(node) = got.next().await {
            let (expect_cid, expect_bytes) = expect
                .get(i)
                .expect("too many nodes in balanced tree stream")
                .clone();
            let (got_cid, got_bytes) = node.expect("unexpected error in balanced tree stream");
            println!("node index {i}");
            assert_eq!(expect_cid, got_cid);
            assert_eq!(expect_bytes, got_bytes);
            i += 1;
        }
        if expect.len() != i {
            panic!(
                "expected at {} nodes of the stream, got {}",
                expect.len(),
                i
            );
        }
    }

    #[tokio::test]
    async fn balanced_tree_test_leaf() {
        let num_chunks = 1;
        let expect = build_expect(num_chunks, 3).await;
        let got = stream_balanced_tree(test_chunk_stream(1), 3);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }

    #[tokio::test]
    async fn balanced_tree_test_height_one() {
        let num_chunks = 3;
        let degrees = 3;
        let expect = build_expect(num_chunks, degrees).await;
        let got = stream_balanced_tree(test_chunk_stream(num_chunks), degrees);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }

    #[tokio::test]
    async fn balanced_tree_test_height_two_full() {
        let degrees = 3;
        let num_chunks = 9;
        let expect = build_expect(num_chunks, degrees).await;
        let got = stream_balanced_tree(test_chunk_stream(num_chunks), degrees);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }

    #[tokio::test]
    async fn balanced_tree_test_height_two_not_full() {
        let degrees = 3;
        let num_chunks = 10;
        let expect = build_expect(num_chunks, degrees).await;
        let got = stream_balanced_tree(test_chunk_stream(num_chunks), degrees);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }

    #[tokio::test]
    async fn balanced_tree_test_height_three() {
        let num_chunks = 125;
        let degrees = 5;
        let expect = build_expect(num_chunks, degrees).await;
        let got = stream_balanced_tree(test_chunk_stream(num_chunks), degrees);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }

    #[tokio::test]
    async fn balanced_tree_test_large() {
        let num_chunks = 780;
        let degrees = 11;
        let expect = build_expect(num_chunks, degrees).await;
        let got = stream_balanced_tree(test_chunk_stream(num_chunks), degrees);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }

    #[tokio::test]
    async fn balanced_tree_test_lar() {
        let num_chunks = 7;
        let degrees = 2;
        let expect = build_expect(num_chunks, degrees).await;
        let got = stream_balanced_tree(test_chunk_stream(num_chunks), degrees);
        tokio::pin!(got);
        ensure_equal(expect, got).await;
    }
}
