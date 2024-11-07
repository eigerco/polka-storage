# Launching the Storage Provider

> This guide assumes you have read the [Building](./building/index.md)
> and the [Local Testnet - Polka Storage](./local-testnet.md) guides
> and have a running testnet to connect to.

Setting up the Storage Provider doesn't have a lot of science, but isn't automatic either!
In this guide, we'll cover how to get up and running with the Storage Provider.

## Registering the Storage Provider

Logically, if you want to participate in the network, you need to register.
To do so, you need to run one of the following commands:

```bash
storagext-cli --sr25519-key <KEY> storage-provider register "<peer_id>"
storagext-cli --ed25519-key <KEY> storage-provider register "<peer_id>"
storagext-cli --ecdsa-key <KEY> storage-provider register "<peer_id>"
```

Where `<KEY>` has been replaced accordingly to its key type.
`<peer_id>` can be anything as it is currently used as a placeholder.

For example: `storagext-cli --sr25519 "//Charlie" storage-provider register "placeholder"`

After registering, you're ready to move on.

## Launching the server 🚀

Similarly to the previous steps, here too you'll need to run a command.
The following is the *minimal* command:

```bash
polka-storage-provider-server \
  --seal-proof 2KiB \
  --post-proof 2KiB \
  --X-key <KEY>
```

Where `--X-key <KEY>` matches the key you used to register yourself with the network, in the previous step.
Note that currently, `--seal-proof` and `--post-proof` only support `2KiB`.

When ran like this, the server will assume a random directory for the database and the storage, however,
you can change that through the `--database-directory` and `--storage-directory`, respectively,
if the directory does not exist, it will be created.

You can also change the parachain node address it connects to,
by default, the server will try to connect to `ws://127.0.0.1:42069`,
but you can change this using the `--node-url` flag.

Finally, you can change the listening addresses for the RPC and HTTP services,
they default to `127.0.0.1:8000` and `127.0.0.1:8001` respectively,
and can be changed using the flags `--rpc-listen-address` and `--upload-listen-address`.

For more information on the available flags, refer to the [`server` chapter](../storage-provider-cli/server.md).
