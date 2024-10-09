use core::cmp::{max, min};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Main part of the seed used for calculting parents of a node.
/// The seed is 32 bytes, 28 bytes are shared between nodes, last 4 bytes is a node id.
/// Reference:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L90>
pub type BucketGraphSeed = [u8; 28];

/// The base degree used for all DRG graphs. One degree from this value is used to ensure that a
/// given node always has its immediate predecessor as a parent, thus ensuring unique topological
/// ordering of the graph nodes.
/// Reference:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L25-L28>
pub const BASE_DEGREE: usize = 6;

/// A Depth Robust Graph constructed via [Alwen et. al](https://acmccs.github.io/papers/p1001-alwenA.pdf>) DR Sample algorithm.
/// References:
/// * <https://spec.filecoin.io/algorithms/porep-old/stacked_drg/#section-algorithms.porep-old.stacked_drg.bucketsample-depth-robust-graphs-algorithm>
/// * <https://acmccs.github.io/papers/p1001-alwenA.pdf>
/// * <https://eprint.iacr.org/2018/678.pdf>
/// * <http://web.archive.org/web/20220623220540/https://web.stanford.edu/~bfisch/porep_short.pdf>
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L111>
pub struct BucketGraph {
    base_degree: usize,
    seed: BucketGraphSeed,
    nodes: usize,
}

impl BucketGraph {
    /// Creates a new BucketGraph initialized with seed.
    ///
    /// It doesn't perform any calculations other than sanity checks.
    ///
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L217>
    pub fn new(nodes: usize, seed: BucketGraphSeed) -> Result<Self, BucketGraphError> {
        // The number of metagraph nodes must be less than `2u64^54` as to not incur rounding errors
        // when casting metagraph node indexes from `u64` to `f64` during parent generation.
        let m_prime = BASE_DEGREE - 1;
        let n_metagraph_nodes = nodes as u64 * m_prime as u64;
        if n_metagraph_nodes > 1u64 << 54 {
            return Err(BucketGraphError::TooManyMetagraphNodes(n_metagraph_nodes));
        }

        Ok(Self {
            base_degree: BASE_DEGREE,
            seed,
            nodes,
        })
    }

    /// Returns a sorted list of all parents of this node. The parents may be repeated.
    ///
    /// If a node doesn't have any parents, then this vector needs to return a vector where
    /// the first element is the requested node. This will be used as indicator for nodes
    /// without parents.
    ///
    /// The `parents` parameter is used to store the result. This is done for performance
    /// reasons, so that the vector can be allocated outside this call.
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L138>
    #[inline]
    pub fn parents(&self, node: usize, parents: &mut [u32]) {
        let m = self.base_degree;

        match node {
            // There are special cases for the first and second node: the first node self
            // references, the second node only references the first node.
            0 | 1 => {
                // Use the degree of the current graph (`m`) as `parents.len()` might be bigger than
                // that (that's the case for Stacked Graph).
                for parent in parents.iter_mut().take(m) {
                    *parent = 0;
                }
            }
            _ => {
                // DRG node indexes are guaranteed to fit within a `u32`.
                let node = node as u32;

                let mut seed = [0u8; 32];
                seed[..28].copy_from_slice(&self.seed);
                seed[28..].copy_from_slice(&node.to_le_bytes());
                let mut rng = ChaCha8Rng::from_seed(seed);

                let m_prime = m - 1;
                // Large sector sizes require that metagraph node indexes are `u64`.
                let metagraph_node = node as u64 * m_prime as u64;
                // In Filecoin this is originally (see notes about this method above):
                // let n_buckets = (metagraph_node as f64).log2().ceil() as u64;
                let n_buckets = ceil_log2(metagraph_node);

                let (predecessor_index, other_drg_parents) = (0, &mut parents[1..]);

                for parent in other_drg_parents.iter_mut().take(m_prime) {
                    let bucket_index = (rng.gen::<u64>() % n_buckets) + 1;
                    let largest_distance_in_bucket = min(metagraph_node, 1 << bucket_index);
                    let smallest_distance_in_bucket = max(2, largest_distance_in_bucket >> 1);

                    // Add 1 becuase the number of distances in the bucket is inclusive.
                    let n_distances_in_bucket =
                        largest_distance_in_bucket - smallest_distance_in_bucket + 1;

                    let distance =
                        smallest_distance_in_bucket + (rng.gen::<u64>() % n_distances_in_bucket);

                    let metagraph_parent = metagraph_node - distance;

                    // Any metagraph node mapped onto the DRG can be safely cast back to `u32`.
                    let mapped_parent = (metagraph_parent / m_prime as u64) as u32;

                    *parent = if mapped_parent == node {
                        node - 1
                    } else {
                        mapped_parent
                    };
                }

                // Immediate predecessor must be the first parent, so hashing cannot begin early.
                parents[predecessor_index] = node - 1;
            }
        }
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-core/src/drgraph.rs#L202C5-L205C6>
    #[inline]
    pub fn size(&self) -> usize {
        self.nodes
    }

    #[inline]
    pub const fn degree(&self) -> usize {
        BASE_DEGREE
    }
}

pub enum BucketGraphError {
    /// Number of nodes in the graph is too big to construct a metagraph with degree [`BASE_DEGREE`].
    /// The number of metagraph nodes must be less than `2u64^54` as to not incur rounding errors
    /// when casting metagraph node indexes from `u64` to `f64` during parent generation.
    TooManyMetagraphNodes(u64),
}

impl core::fmt::Debug for BucketGraphError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            BucketGraphError::TooManyMetagraphNodes(nodes) => {
                write!(f, "too many metagraph nodes {}", nodes)
            }
        }
    }
}

/// In Filecoin the computation `(n_u64 as f64).log2().ceil() as u64` is need. Currently, in Rust
/// it is not possible to compute f64::log2 in `no-std` environment. This method is an alternative
/// implementation.
pub(crate) const fn ceil_log2(n: u64) -> u64 {
    if n == 0 {
        0
    } else {
        let n_new = n - 1u64;
        (u64::BITS - n_new.leading_zeros()) as u64
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ceil_log2_computation_same_as_filecoin() {
        for n in 0..10001 {
            let n_u64 = n as u64;
            let n_buckets_fc = (n_u64 as f64).log2().ceil() as u64;
            let n_buckets_we = crate::graphs::bucket::ceil_log2(n_u64);
            assert_eq!(n_buckets_fc, n_buckets_we);
        }
    }

    #[test]
    fn constructs_graph() {
        for &nodes in &[4, 16, 256, 2048] {
            let g = BucketGraph::new(nodes, [7u8; 28]).unwrap();

            let mut parents = vec![0; BASE_DEGREE];
            g.parents(0, &mut parents);
            assert_eq!(parents, vec![0; BASE_DEGREE], "first node self references");
            parents = vec![0; BASE_DEGREE];
            g.parents(1, &mut parents);
            assert_eq!(
                parents,
                vec![0; BASE_DEGREE],
                "second node references only the first node"
            );

            for i in 1..nodes {
                let mut pa1 = vec![0; BASE_DEGREE];
                g.parents(i, &mut pa1);
                let mut pa2 = vec![0; BASE_DEGREE];
                g.parents(i, &mut pa2);

                assert_eq!(pa1.len(), BASE_DEGREE);
                assert_eq!(
                    pa1, pa2,
                    "parents called on the same node twice are the same"
                );

                let mut p1 = vec![0; BASE_DEGREE];
                g.parents(i, &mut p1);

                for parent in p1 {
                    assert_ne!(
                        i, parent as usize,
                        "there are no self references in parents"
                    );
                }

                assert_eq!(
                    i - 1,
                    pa1[0] as usize,
                    "immediate predecessor is not a first parent"
                );
            }
        }
    }
}
