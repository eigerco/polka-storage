# Polka Storage Provider

The Polka Storage Provider CLI provides two commands — `wallet` and `storage`.

## `storage`

The `storage` command launches a server that converts arbitrary files into CARv2 files
— it does so using our CARv2 Rust library (the first Rust CARv2 implementation).

You can read more about it on the [`storage`](./storage.md) page.

## `wallet`

The `wallet` command is a thin wrapper over the [`subkey`](https://docs.substrate.io/reference/command-line-tools/subkey/) utility provided by Polkadot.

You can read more about it on the [`wallet`](./wallet.md) page.
