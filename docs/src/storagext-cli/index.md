# Storagext CLI

Alongside the pallets, we've also developed a CLI to enable calling extrinsics without Polkadot.js.

The CLI's goal is to ease development and testing and to sidestep some limitations of the Polkadot.js visual interface.

This chapter covers how to use the `storagext-cli`, along with that,
there are several usage examples available throughout the book.

## Getting started

The `storagext-cli` takes two main flags — the node's RPC address and a key[^keys],
the latter is split into three kinds, and **one is required** for most operations
(for example, if the operation being called is a [signed extrinsic](https://wiki.polkadot.network/docs/learn-transactions#types-of-extrinsics)[^optional_keys]):

- [Sr25519](https://wiki.polkadot.network/docs/learn-cryptography) — `--sr25519-key` or the `SR25519_KEY` environment variable
- [ECDSA](https://en.bitcoin.it/wiki/Secp256k1) — `--ecdsa-key` or the `ECDSA_KEY` environment variable
- [Ed25519](https://en.wikipedia.org/wiki/EdDSA#Ed25519) — `--ed25519-key` or the `ED25519_KEY` environment variable

For example, to connect to a node at `supercooldomain.com:1337` using Charlie's Sr25519 key:

```bash
storagext-cli --node-rpc "supercooldomain.com:1337" --sr25519-key "//Charlie" <commands>
```

Or, retrieving the same key but using the environment variable form:

```bash
SR25519_KEY="//Charlie" storagext-cli --node-rpc "supercooldomain.com:1337" <commands>
```

### Flags

| Name                      | Description                                                                                        |
| ------------------------- | -------------------------------------------------------------------------------------------------- |
| `--node-rpc`              | The node's RPC address (including port), defaults to `ws://127.0.0.1:42069`                        |
| `--sr25519-key`           | Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Charlie`                           |
| `--ecdsa-key`             | ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Charlie`                             |
| `--ed25519-key`           | Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Charlie`                           |
| `--format`                | The output format, either `json` or `plain` (case insensitive), defaults to `plain`                |
| `--n-retries`             | The number of connection retries when trying to initially connect to the parachain, defaults to 10 |
| `--retry-interval`        | The retry interval between connection retries, in milliseconds, defaults to 3000 (3 seconds)       |
| `--wait-for-finalization` | Wait for the inclusion of the extrinsic call in a finalized block, default behavior is to not wait |

#### `--format`

The `--format` global flag changes how extrinsic output is done, if the output is set to `plain`
we **do not** make any guarantees about the output format, as such, you should not rely on it for scripts!

If the output is set to `--json` all standard output from the CLI will be formatted as JSON, however,
as it **currently** stands, we do not guarantee a stable interface — though we will make an effort to keep changes to a minimum, and document them.

[^keys]: Read more about how cryptographic keys are used in Polkadot — <https://wiki.polkadot.network/docs/learn-cryptography>.
[^optional_keys]: If a key is passed to the CLI, but the operation called does not require a key, **the key will not be used**.

#### `--n-retries` and `--retry-interval`

These flags help you connect under a difficult network environment, or when you're launching the node and it's still booting up,
this allows you to "actively wait" for the node to come online.

#### `--wait-for-finalization`

If you want to see the result of your extrinsic call, this flag is for you.
By default, `storagext-cli` will not wait for the result of the extrinsic,
when this flag is enabled, `storagext-cli` will wait until the extrinsic makes it to a finalized block
and will report it's result — whether the call was successful or not.

## Sub-chapters

- [Subcommand `market`](market.md)
- [Subcommand `storage-provider`](storage-provider.md)
- [Subcommand `system`](system.md)
