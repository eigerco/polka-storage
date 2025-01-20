# Polka Storage Provider — Server

This chapter covers the available CLI options for the Polka Storage Provider server.

<!-- Sadly, tables will not cut it here, since the text is just too big for the table. -->

### `--sr25519-key`

Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

See [`sp_core::crypto::Pair::from_string_with_seed`](https://docs.rs/sp-core/latest/sp_core/crypto/trait.Pair.html#method.from_string_with_seed) for more information.

If this `--sr25519-key` is not used, either [`--ecdsa-key`](#--ecdsa-key) or [`--ed25519-key`](#--ed25519-key) MUST be used.

### `--ecdsa-key`

ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

See [`sp_core::crypto::Pair::from_string_with_seed`](https://docs.rs/sp-core/latest/sp_core/crypto/trait.Pair.html#method.from_string_with_seed) for more information.

If this `--ecdsa-key` is not used, either [`--sr25519-key`](#--sr25519-key) or [`--ed25519-key`](#--ed25519-key) MUST be used.

### `--ed25519-key`

Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

See [`sp_core::crypto::Pair::from_string_with_seed`](https://docs.rs/sp-core/latest/sp_core/crypto/trait.Pair.html#method.from_string_with_seed) for more information.

If this `--ed25519-key` is not used, either [`--ecdsa-key`](#--ecdsa-key) or [`--sr25519-key`](#--sr25519-key) MUST be used.

### `--upload-listen-address`

The storage server's endpoint address — i.e. where the client will upload their files to.

It takes in an IP address along with a port in the format: `<ip>:<port>`.
Defaults to `127.0.0.1:8001`.

### `--rpc-listen-address`

The RPC server endpoint's address — i.e. where you will submit your deals to.

It takes in an IP address along with a port in the format: `<ip>:<port>`.
Defaults to `127.0.0.1:8000`.

### `--node-url`

The target parachain node's address — i.e. the parachain node the storage provider will submit deals to, etc.

It takes in an URL, it supports both HTTP and WebSockets and their secure variants.
Defaults to `ws://127.0.0.1:42069`.

### `--database-directory`

The RocksDB storage directory, where deal information will be kept.

It takes in a valid folder path, if the directory does not exist, it will be created along with all intermediate paths.
Defaults to a pseudo-random temporary directory — `/tmp/<random string>/deals_database`.

### `--storage-directory`

The piece storage directory, where pieces will be kept.

It takes in a valid folder path, if the directory does not exist, it will be created along with all intermediate paths.
Defaults to a pseudo-random temporary directory — `/tmp/<random string>/...`.

Storage directories for the pieces, unsealed and sealed sectors will be created under it.

### `--seal-proof`

The kind of replication proof. Currently, only `StackedDRGWindow2KiBV1P1` is supported to which it defaults.

### `--post-proof`

The kind of storage proof. Currently, only `StackedDRGWindow2KiBV1P1` is supported to which it defaults.

### `--porep-parameters`

The path to the PoRep proving parameters. They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.

### `--post-parameters`

The path to the PoSt proving parameters. They are shared across all of the nodes in the network, as the chain stores corresponding Verifying Key parameters.

### `--node-type`

The P2P node type that the server will be running. Can be either `bootstrap` or `registration` node.

### `--p2p-key`

The ED25519 private key used by the P2P node.

### `--rendezvous-point-address`

Rendezvous point address that the registration node connects to or the bootstrap node binds to.

### `--rendezvous-point`

Peer ID of the rendezvous point that the registration node connects to. Only needed if running a registration P2P node.

### `--registration-ttl`

The TTL of the p2p registration in seconds. After the node registration expires, the server automatically re-registers itself.

### `--config`

Takes in a path to a configuration file, it supports both JSON and TOML (files _must_ have the right extension).
The supported configuration parameters are:

| Name                       | Default                        |
| -------------------------- | ------------------------------ |
| `upload-listen-address`    | `127.0.0.1:8000`               |
| `rpc-listen-address`       | `127.0.0.1:8001`               |
| `node-url`                 | `ws://127.0.0.1:42069`         |
| `database-directory`       | `/tmp/<random>/deals_database` |
| `storage-directory`        | `/tmp/<random>/deals_storage`  |
| `seal-proof`               | `2KiB`                         |
| `post-proof`               | `2KiB`                         |
| `porep_parameters`         | NA                             |
| `post_parameters`          | NA                             |
| `node_type`                | `bootstrap`                    |
| `p2p_key`                  | NA                             |
| `rendezvous_point_address` | NA                             |
| `rendezvous_point`         | `None`                         |
| `registration_ttl`         | `None`                         |

#### Bare bones configuration

```json
{
    "porep_parameters": "/home/storage_provider/porep.params",
    "post_parameters": "/home/storage_provider/post.params",
    "p2p_key": "/home/storage_provider/private_key.pem",
    "rendezvous_point_address": "/ip4/127.0.0.1/tcp/62649",
}
```
