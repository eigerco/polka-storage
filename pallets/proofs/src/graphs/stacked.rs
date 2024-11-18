use crate::{crypto::feistel, graphs::bucket::BucketGraph};

/// Expansion degree used for Stacked Graphs.
///
/// References:
/// * <https://github.com/filecoin-project/research/issues/144>
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L27>
pub const EXP_DEGREE: usize = 8;

/// Zig-Zag graph constructed via Chung's construction and pseudo-random function.
///
/// References:
/// * <https://www.youtube.com/watch?v=8_9ONpyRZEI>
/// * <https://github.com/filecoin-project/research/issues/144>
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/proof_scheme.rs#L27>
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L32>
pub struct StackedBucketGraph {
    base_graph: BucketGraph,
    feistel_keys: [feistel::Index; 4],
    feistel_precomputed: feistel::Precomputed,
}

impl StackedBucketGraph {
    pub fn new(base_graph: BucketGraph, feistel_keys: [feistel::Index; 4]) -> Self {
        let size = base_graph.size();
        Self {
            base_graph,
            feistel_keys,
            feistel_precomputed: feistel::precompute((EXP_DEGREE * size) as feistel::Index),
        }
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L420>
    pub fn base_parents(&self, node: usize, parents: &mut [u32]) {
        // No cache usage, generate on demand.
        self.base_graph.parents(node, parents)
    }

    /// Assign `self.expansion_degree` parents to `node` using an invertible permutation
    /// that is applied one way for the forward layers and one way for the reversed
    /// ones.
    ///
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L388>
    #[inline]
    pub fn expanded_parents(&self, node: usize, expanded_parents: &mut [u32]) {
        debug_assert_eq!(expanded_parents.len(), EXP_DEGREE);
        for (i, el) in expanded_parents.iter_mut().enumerate() {
            *el = self.correspondent(node, i);
        }
    }

    /// Assign one parent to `node` using a Chung's construction with a reversible
    /// permutation function from a Feistel cipher (controlled by `invert_permutation`).
    ///
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L341>
    fn correspondent(&self, node: usize, i: usize) -> u32 {
        // We can't just generate random values between `[0, size())`, we need to
        // expand the search space (domain) to accommodate every unique parent assignment
        // generated here. This can be visualized more clearly as a matrix where the each
        // new parent of each new node is assigned a unique `index`:
        //
        //
        //          | Parent 1 | Parent 2 | Parent 3 |
        //
        // | Node 1 |     0    |     1    |     2    |
        //
        // | Node 2 |     3    |     4    |     5    |
        //
        // | Node 3 |     6    |     7    |     8    |
        //
        // | Node 4 |     9    |     A    |     B    |
        //
        // This starting `index` will be shuffled to another position to generate a
        // parent-child relationship, e.g., if generating the parents for the second node,
        // `permute` would be called with values `[3; 4; 5]` that would be mapped to other
        // indexes in the search space of `[0, B]`, say, values `[A; 0; 4]`, that would
        // correspond to nodes numbered `[4; 1, 2]` which will become the parents of the
        // second node. In a later pass invalid parents like 2, self-referencing, and parents
        // with indexes bigger than 2 (if in the `forward` direction, smaller than 2 if the
        // inverse), will be removed.
        let a = (node * EXP_DEGREE) as feistel::Index + i as feistel::Index;

        let transformed = feistel::permute(
            self.size() as feistel::Index * EXP_DEGREE as feistel::Index,
            a,
            &self.feistel_keys,
            self.feistel_precomputed,
        );

        // Collapse the output in the matrix search space to the row of the corresponding
        // node (losing the column information, that will be regenerated later when calling
        // back this function in the `reversed` direction).
        u32::try_from(transformed / EXP_DEGREE as u64).expect("invalid transformation")
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L286C5-L288C6>
    pub fn size(&self) -> usize {
        self.base_graph.size()
    }

    pub fn base_degree(&self) -> usize {
        self.base_graph.degree()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::graphs::bucket::BucketGraph;

    /// Tests that the set of expander edges has not been truncated.
    /// Reference:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L598>
    #[test]
    fn test_high_parent_bits() {
        // 64GiB sectors have 2^31 nodes.
        const N_NODES: usize = 1 << 31;

        // `u32` truncation would reduce the expander edge bit-length from 34 bits to 32 bits, thus
        // the first parent truncated would be the node at index `2^32 / EXP_DEGREE = 2^29`.
        const FIRST_TRUNCATED_PARENT: u32 = 1 << 29;

        // The number of child nodes to test before failing. This value was chosen arbitrarily and
        // can be changed.
        const N_CHILDREN_SAMPLED: usize = 3;

        let base_graph = BucketGraph::new(N_NODES, [0u8; 28]).unwrap();
        let graph = StackedBucketGraph::new(base_graph, [0, 1, 2, 3]);

        let mut exp_parents = [0u32; EXP_DEGREE];
        for v in 0..N_CHILDREN_SAMPLED {
            graph.expanded_parents(v, &mut exp_parents[..]);
            if exp_parents.iter().any(|u| *u >= FIRST_TRUNCATED_PARENT) {
                return;
            }
        }
        panic!();
    }

    /// Checks that the distribution of parent node indexes within a sector is within a set bound.
    /// Reference:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/src/stacked/vanilla/graph.rs#L637>
    #[test]
    fn test_exp_parent_histogram() {
        // 64GiB sectors have 2^31 nodes.
        const N_NODES: usize = 1 << 31;

        // The number of children used to construct the histogram. This value is chosen
        // arbitrarily and can be changed.
        const N_CHILDREN_SAMPLED: usize = 10000;

        // The number of bins used to partition the set of sector nodes. This value was chosen
        // arbitrarily and can be changed to any integer that is a multiple of `EXP_DEGREE` and
        // evenly divides `N_NODES`.
        const N_BINS: usize = 32;
        const N_NODES_PER_BIN: u32 = (N_NODES / N_BINS) as u32;
        const PARENT_COUNT_PER_BIN_UNIFORM: usize = N_CHILDREN_SAMPLED * EXP_DEGREE / N_BINS;

        // This test will pass if every bin's parent count is within the bounds:
        // `(1 +/- FAILURE_THRESHOLD) * PARENT_COUNT_PER_BIN_UNIFORM`.
        const FAILURE_THRESHOLD: f32 = 0.4;
        const MAX_PARENT_COUNT_ALLOWED: usize =
            ((1.0 + FAILURE_THRESHOLD) * PARENT_COUNT_PER_BIN_UNIFORM as f32) as usize - 1;
        const MIN_PARENT_COUNT_ALLOWED: usize =
            ((1.0 - FAILURE_THRESHOLD) * PARENT_COUNT_PER_BIN_UNIFORM as f32) as usize + 1;

        // Non-legacy porep-id.

        let base_graph = BucketGraph::new(N_NODES, [0u8; 28]).unwrap();
        let graph = StackedBucketGraph::new(base_graph, [0, 1, 2, 3]);

        // Count the number of parents in each bin.
        let mut hist = [0usize; N_BINS];
        let mut exp_parents = [0u32; EXP_DEGREE];
        for sample_index in 0..N_CHILDREN_SAMPLED {
            let v = sample_index * N_NODES / N_CHILDREN_SAMPLED;
            graph.expanded_parents(v, &mut exp_parents[..]);
            for u in exp_parents.iter() {
                let bin_index = (u / N_NODES_PER_BIN) as usize;
                hist[bin_index] += 1;
            }
        }

        let success = hist.iter().all(|&n_parents| {
            (MIN_PARENT_COUNT_ALLOWED..=MAX_PARENT_COUNT_ALLOWED).contains(&n_parents)
        });

        assert!(success);
    }
}
