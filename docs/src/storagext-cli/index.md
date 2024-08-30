# Storagext CLI

Alongside the pallets, we've also developed a CLI to enable calling extrinsics without Polkadot.js.

The CLI's goal is to ease development and testing and to sidestep some limitations of the Polkadot.js visual interface.

This chapter covers how to use the `storagext-cli`, and you will find several examples of usage throughout the book.

## Getting started

The `storagext-cli` takes two main flags — the node's RPC address and a key[^keys],
the latter is split into three kinds, and **one is required** for most operations
(for example, if the operation being called is a [signed extrinsic](https://wiki.polkadot.network/docs/learn-transactions#types-of-extrinsics)[^optional_keys]):

- [Sr25519](https://wiki.polkadot.network/docs/learn-cryptography) — `--sr25519-key` or the `SR25519_KEY` environment variable
- [ECDSA](https://en.bitcoin.it/wiki/Secp256k1) — `--ecdsa-key` or the `ECDSA_KEY` environment variable
- [Ed25519](https://en.wikipedia.org/wiki/EdDSA#Ed25519) — `--ed25519-key` or the `ED25519_KEY` environment variable

For example, to connect to a node at `supercooldomain.com:1337` using Charlie's Sr25519 key, you would run the following command:

```bash
storagext-cli --node-rpc "supercooldomain.com:1337" --sr25519-key "//Charlie" <commands>
```

Or, retrieving the same key but using the environment variable form:

```bash
SR25519_KEY="//Charlie" storagext-cli --node-rpc "supercooldomain.com:1337" <commands>
```

### Flags

| Name            | Description                                                                 |
| --------------- | --------------------------------------------------------------------------- |
| `--node-rpc`    | The node's RPC address (including port), defaults to `ws://127.0.0.1:42069` |
| `--sr25519-key` | Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Charlie`    |
| `--ecdsa-key`   | ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Charlie`      |
| `--ed25519-key` | Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Charlie`    |

[^keys]: Read more about how cryptographic keys are used in Polkadot — <https://wiki.polkadot.network/docs/learn-cryptography>.
[^optional_keys]: If a key is passed to the CLI, but the operation called does not require a key, **the key will not be used**.

## Sub-chapters

- [Subcommand `market`](market.md)
- [Subcommand `storage-provider`](storage-provider.md)
- [Subcommand `system`](system.md)
