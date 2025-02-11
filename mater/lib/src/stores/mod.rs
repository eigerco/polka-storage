mod blockstore;
mod file;
mod filestore;

pub use blockstore::Blockstore;
pub use file::FileBlockstore;
pub use filestore::create_filestore;

/// The default block size, as defined in
/// [boxo](https://github.com/ipfs/boxo/blob/f4fe8997dcbeb39b3a4842d8f08b34739bfd84a4/chunker/parse.go#L13).
pub const DEFAULT_CHUNK_SIZE: usize = 256 * 1024;

/// The default tree width, also called links per block, as defined in
/// [boxo](https://github.com/ipfs/boxo/blob/625ba769263c2beeec934836f54bbd6624db945a/ipld/unixfs/importer/helpers/helpers.go#L16-L30).
pub const DEFAULT_TREE_WIDTH: usize = 174;

/// Store configuration options.
pub enum Config {
    /// The store should use the balanced tree layout,
    /// generating byte chunks of `chunk_size` and
    /// generating parent nodes every `tree_width` nodes.
    Balanced {
        /// The size of the byte chunks.
        chunk_size: usize,
        /// The number of children per parent node.
        tree_width: usize,
        /// If false it's unixfs
        raw: bool
    },
}

impl Config {
    /// Create a new [`Config::Balanced`].
    pub fn balanced(chunk_size: usize, tree_width: usize, raw: bool) -> Self {
        Self::Balanced {
            chunk_size,
            tree_width,
            raw
        }
    }

    /// Creates a new balanced tree configuration with UnixFS wrapping (recommended).
    pub fn balanced_unixfs(chunk_size: usize, tree_width: usize) -> Self {
        Self::balanced(chunk_size, tree_width, false)
    }

    /// Creates a new balanced tree configuration with raw storage.
    pub fn balanced_raw(chunk_size: usize, tree_width: usize) -> Self {
        Self::balanced(chunk_size, tree_width, true)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::Balanced {
            chunk_size: DEFAULT_CHUNK_SIZE,
            tree_width: DEFAULT_TREE_WIDTH,
            raw: false
        }
    }
}
