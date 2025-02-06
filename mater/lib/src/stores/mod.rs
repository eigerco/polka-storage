mod blockstore;
mod file;
mod filestore;

pub use blockstore::Blockstore;
pub use file::FileBlockstore;
pub use filestore::create_filestore;

/// The default chunk size for balanced trees (256 KiB)
/// Reference: https://github.com/ipfs/boxo/blob/f4fe8997dcbeb39b3a4842d8f08b34739bfd84a4/chunker/parse.go#L13
pub const DEFAULT_CHUNK_SIZE: usize = 256 * 1024;

/// The default number of children per parent node in balanced trees.
/// This value comes from the go-ipfs implementation and provides a good balance
/// between tree depth and width for most use cases.
pub const DEFAULT_TREE_WIDTH: usize = 174;

/// Store configuration options for controlling how data is stored and structured.
#[derive(Debug, Clone)]
pub enum Config {
    /// The store should use the balanced tree layout,
    /// generating byte chunks of `chunk_size` and
    /// generating parent nodes every `tree_width` nodes.
    Balanced {
        /// The size of the byte chunks.
        chunk_size: usize,
        /// The number of children per parent node.
        tree_width: usize,

        /// If true, store content directly without UnixFS metadata.
        raw_mode: bool,
    },
}

impl Config {
    /// Creates a new balanced tree configuration with the specified parameters.
    ///
    /// # Arguments
    /// * `chunk_size` - Size of each data chunk in bytes
    /// * `tree_width` - Maximum number of children per parent node
    /// * `raw_mode` - Whether to store content directly without UnixFS wrapping. Raw is more space efficient but loses IPFS compatibility features. Default is false (uses UnixFS wrapping).
    pub fn balanced(chunk_size: usize, tree_width: usize, raw_mode: bool) -> Self {
        Self::Balanced {
            chunk_size,
            tree_width,
            raw_mode,
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
            raw_mode: false, // Default to UnixFS wrapping for IPFS compatibility
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        match config {
            Config::Balanced {
                chunk_size,
                tree_width,
                raw_mode,
            } => {
                assert_eq!(chunk_size, DEFAULT_CHUNK_SIZE);
                assert_eq!(tree_width, DEFAULT_TREE_WIDTH);
                assert!(!raw_mode);
            }
        }
    }

    #[test]
    fn test_balanced_unixfs_config_builder() {
        let chunk_size = 1024;
        let tree_width = 10;

        let Config::Balanced {
            chunk_size: cs,
            tree_width: tw,
            raw_mode,
        } = Config::balanced_unixfs(chunk_size, tree_width);
        assert_eq!(cs, chunk_size);
        assert_eq!(tw, tree_width);
        assert!(!raw_mode);
    }

    #[test]
    fn test_balanced_raw_config_builder() {
        let chunk_size = 1024;
        let tree_width = 10;

        let Config::Balanced {
            chunk_size: cs,
            tree_width: tw,
            raw_mode,
        } = Config::balanced_raw(chunk_size, tree_width);
        assert_eq!(cs, chunk_size);
        assert_eq!(tw, tree_width);
        assert!(raw_mode);
    }
}
