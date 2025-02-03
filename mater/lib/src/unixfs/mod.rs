//! UnixFS implementation based on
//! <https://github.com/n0-computer/beetle/blob/3e137cb2bc18e1d458c3f72d5e817b03d9537d5d/iroh-unixfs/src/balanced_tree.rs>.

mod unixfs_pb;
pub use unixfs_pb::{mod_Data, Data};

use std::collections::{HashMap, VecDeque};

use crate::{
    multicodec::{generate_multihash, DAG_PB_CODE, RAW_CODE},
    Config, Error,
};
use async_stream::try_stream;
use bytes::Bytes;
use futures::TryStreamExt;
use futures::{Stream, StreamExt};
use ipld_core::{cid::Cid, codec::Codec};
use ipld_dagpb::{DagPbCodec, PbLink, PbNode};
use quick_protobuf::MessageWrite;
use sha2::Sha256;

#[derive(Debug, Clone, Copy)]
struct LinkInfo {
    raw_data_length: u64,
    encoded_data_length: u64,
}

#[derive(Debug)]
enum TreeNode {
    Leaf(Bytes),
    Stem(Vec<(Cid, LinkInfo)>),
}

impl TreeNode {
    fn encode_unixfs_leaf_node(chunk: &Bytes) -> Result<((Cid, Bytes), LinkInfo), Error> {
        let chunk_len = chunk.len() as u64;

        // Build UnixFS metadata
        let unixfs_data = Data {
            Type: mod_Data::DataType::File,
            filesize: Some(chunk_len),
            blocksizes: vec![chunk_len],
            Data: Some(chunk.to_vec().into()),
            hashType: None,
            fanout: None,
        };

        // Encode UnixFS data and create DAG-PB node
        let mut data_buf = Vec::new();
        {
            let mut w = quick_protobuf::Writer::new(&mut data_buf);
            unixfs_data.write_message(&mut w)?;
        }

        let pb_node = PbNode {
            links: vec![],
            data: Some(data_buf.clone().into()),
        };

        let encoded = DagPbCodec::encode_to_vec(&pb_node)?;
        let mh = generate_multihash::<Sha256, _>(&encoded);
        let cid = Cid::new_v1(DAG_PB_CODE, mh);

        let info = LinkInfo {
            raw_data_length: chunk_len,
            encoded_data_length: encoded.len() as u64,
        };

        Ok(((cid, encoded.into()), info))
    }

    fn encode_unixfs_stem_node(
        children: Vec<(Cid, LinkInfo)>,
    ) -> Result<((Cid, Bytes), LinkInfo), Error> {
        // Process all children in a single pass, gathering totals and building links and blocksizes
        let (total_raw_size, total_encoded_size, pb_links, blocksizes) = children.iter().fold(
            (
                0u64,
                0u64,
                Vec::with_capacity(children.len()),
                Vec::with_capacity(children.len()),
            ),
            |(raw_sum, encoded_sum, mut links, mut sizes), (child_cid, link_info)| {
                sizes.push(link_info.raw_data_length);
                links.push(PbLink {
                    cid: *child_cid,
                    name: Some("".to_string()),
                    size: Some(link_info.encoded_data_length),
                });
                (
                    raw_sum + link_info.raw_data_length,
                    encoded_sum + link_info.encoded_data_length,
                    links,
                    sizes,
                )
            },
        );

        // Create UnixFS metadata
        let unixfs_data = Data {
            Type: mod_Data::DataType::File,
            filesize: Some(total_raw_size),
            blocksizes,
            Data: None,
            hashType: None,
            fanout: None,
        };

        // Encode UnixFS data
        let mut data_buf = Vec::new();
        {
            let mut w = quick_protobuf::Writer::new(&mut data_buf);
            unixfs_data.write_message(&mut w)?;
        }

        // Create DAG-PB node
        let pb_node = PbNode {
            links: pb_links,
            data: Some(data_buf.clone().into()),
        };

        let encoded = DagPbCodec::encode_to_vec(&pb_node)?;
        let mh = generate_multihash::<Sha256, _>(&encoded);
        let cid = Cid::new_v1(DAG_PB_CODE, mh);

        let info = LinkInfo {
            raw_data_length: data_buf.len() as u64,
            encoded_data_length: encoded.len() as u64 + total_encoded_size,
        };

        Ok(((cid, encoded.into()), info))
    }

    fn encode_raw(&self) -> Result<((Cid, Bytes), LinkInfo), Error> {
        match self {
            TreeNode::Leaf(chunk) => {
                let mh = generate_multihash::<Sha256, _>(&chunk);
                let cid = Cid::new_v1(RAW_CODE, mh);
                let info = LinkInfo {
                    raw_data_length: chunk.len() as u64,
                    encoded_data_length: chunk.len() as u64,
                };
                Ok(((cid, chunk.clone()), info))
            }
            TreeNode::Stem(children) => {
                let (total_raw_size, total_encoded_size, pb_links) = children.iter().fold(
                    (0u64, 0u64, Vec::new()),
                    |(raw_sum, encoded_sum, mut links), (child_cid, link_info)| {
                        links.push(PbLink {
                            cid: *child_cid,
                            name: None,
                            size: Some(link_info.encoded_data_length),
                        });
                        (
                            raw_sum + link_info.raw_data_length,
                            encoded_sum + link_info.encoded_data_length,
                            links,
                        )
                    },
                );

                let pb_node = PbNode {
                    links: pb_links,
                    data: None,
                };

                let encoded = DagPbCodec::encode_to_vec(&pb_node)?;
                let mh = generate_multihash::<Sha256, _>(&encoded);
                let cid = Cid::new_v1(DAG_PB_CODE, mh);
                // NOTE(@jmg-duarte,28/05/2024): In the original implementation
                // they have a `Block` structure that contains the child links,
                // we're not currently using them and as such I didn't include them
                let info = LinkInfo {
                    raw_data_length: total_raw_size,
                    encoded_data_length: total_encoded_size + encoded.len() as u64,
                };

                Ok(((cid, encoded.into()), info))
            }
        }
    }
}

pub(crate) fn stream_balanced_tree<'a, I>(
    input: I,
    width: usize,
    config: &'a Config,
) -> impl Stream<Item = Result<(Cid, Bytes), Error>> + 'a
where
    I: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    try_stream! {
        let mut levels: VecDeque<Vec<(Cid, LinkInfo)>> = VecDeque::new();
        levels.push_back(vec![]);

        let mut seen_blocks = HashMap::new();
        let input = input.map_err(Error::from);
        tokio::pin!(input);

        while let Some(data) = input.next().await {
            let chunk = data?;
            let node = TreeNode::Leaf(chunk);

            let ((leaf_cid, leaf_bytes), leaf_info) = match config {
                Config::Balanced { raw_mode, .. } if *raw_mode => node.encode_raw()?,
                _ => match node {
                    TreeNode::Leaf(ref chunk) => TreeNode::encode_unixfs_leaf_node(chunk)?,
                    TreeNode::Stem(ref children) => TreeNode::encode_unixfs_stem_node(children.clone())?,
                },
            };

            if !seen_blocks.contains_key(&leaf_cid) {
                seen_blocks.insert(leaf_cid, leaf_info);
                yield (leaf_cid, Bytes::from(leaf_bytes.to_vec()));
            }

            levels[0].push((leaf_cid, leaf_info));

            for level in 0..levels.len() {
                if levels[level].len() < width {
                    break;
                }

                let children = std::mem::replace(&mut levels[level], Vec::with_capacity(width));
                let stem = TreeNode::Stem(children.clone());

                let ((cid, data), info) = match config {
                    Config::Balanced { raw_mode, .. } if *raw_mode => stem.encode_raw()?,
                    _ => TreeNode::encode_unixfs_stem_node(children)?,
                };

                if !seen_blocks.contains_key(&cid) {
                    seen_blocks.insert(cid, info);
                    yield (cid, Bytes::from(data.to_vec()));
                }

                if level + 1 == levels.len() {
                    levels.push_back(vec![]);
                }
                levels[level + 1].push((cid, info));
            }
        }

        while let Some(leftover) = levels.pop_front() {
            if leftover.is_empty() {
                continue;
            }

            let stem = TreeNode::Stem(leftover.clone());
            let ((cid, data), info) = match config {
                Config::Balanced { raw_mode, .. } if *raw_mode => stem.encode_raw()?,
                _ => TreeNode::encode_unixfs_stem_node(leftover)?,
            };

            if !seen_blocks.contains_key(&cid) {
                seen_blocks.insert(cid, info);
                yield (cid, Bytes::from(data.to_vec()));
            }

            if let Some(up) = levels.front_mut() {
                up.push((cid, info));
            }
        }
    }
}
