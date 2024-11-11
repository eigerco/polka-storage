# Launching the Storage Provider

> This guide assumes you have read the [Building](./building/index.md)
> and the [Local Testnet - Polka Storage](./local-testnet.md) guides
> and have a running testnet to connect to.

Setting up the Storage Provider doesn't have a lot of science, but isn't automatic either!
In this guide, we'll cover how to get up and running with the Storage Provider.

## Generating the PoRep Parameters

First and foremost, to allow the Storage Provider to generate [PoRep](https://docs.filecoin.io/basics/the-blockchain/proofs#proof-of-replication-porep) proofs,
we need to first generate their parameters, we do that with the following command:

```bash
$ polka-storage-provider-client proofs porep-params
Generating params for 2KiB sectors... It can take a couple of minutes ⌛
Generated parameters:
/home/user/polka-storage/2KiB.porep.params
/home/user/polka-storage/2KiB.porep.vk
/home/user/polka-storage/2KiB.porep.vk.scale
```

As advertised, the command has generated the following files:

* `2KiB.porep.params` — The PoRep parameters
* `2KiB.porep.vk` — The verifying key
* `2KiB.porep.vk.scale` - The verifying key, encoded in SCALE format

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

For example: `storagext-cli --sr25519-key "//Charlie" storage-provider register "placeholder"`

After registering, there is one more thing to be done, to be able to verify proofs in local testnet.
We need to set the global verifying key in the network, so it's compatible with the proving parameters:

```bash
storagext-cli --sr25519-key "//Charlie" proofs set-porep-verifying-key 2KiB.vk.scale
```

And you're ready!

## Launching the server 🚀

Similarly to the previous steps, here too you'll need to run a command.
The following is the *minimal* command:

```bash
polka-storage-provider-server \
  --seal-proof 2KiB \
  --post-proof 2KiB \
  --porep-parameters <POREP-PARAMS> \
  --X-key <KEY>
```

Where `--X-key <KEY>` matches the key type you used to register yourself with the network, in the previous step.
Note that currently, `--seal-proof` and `--post-proof` only support `2KiB`.

`<POREP-PARAMS>` is the resulting `*.porep.params` file from the [first steps](#generating-the-porep-parameters),
in this case, `2KiB.porep.params`.

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
