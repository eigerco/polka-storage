mod blockstore;
mod filestore;

pub use blockstore::Blockstore;
pub use filestore::Filestore;

/// The default block size, as defined in
/// [boxo](https://github.com/ipfs/boxo/blob/f4fe8997dcbeb39b3a4842d8f08b34739bfd84a4/chunker/parse.go#L13).
pub(crate) const DEFAULT_BLOCK_SIZE: usize = 1024 * 256;

/// The default tree width, also called links per block, as defined in
/// [boxo](https://github.com/ipfs/boxo/blob/625ba769263c2beeec934836f54bbd6624db945a/ipld/unixfs/importer/helpers/helpers.go#L16-L30).
pub(crate) const DEFAULT_TREE_WIDTH: usize = 174;

/// Store configuration options.
pub enum Config {
    /// The store should use the balanced tree layout,
    /// generating byte chunks of `chunk_size` and
    /// generating parent nodes every `tree_width` nodes.
    Balanced {
        chunk_size: usize,
        tree_width: usize,
    },
}

impl Config {
    /// Create a new [`Config::Balanced`].
    pub fn balanced(chunk_size: usize, tree_width: usize) -> Self {
        Self::Balanced {
            chunk_size,
            tree_width,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::Balanced {
            chunk_size: DEFAULT_BLOCK_SIZE,
            tree_width: DEFAULT_TREE_WIDTH,
        }
    }
}
