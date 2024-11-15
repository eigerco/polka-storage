# client

We cover the commands provided by the `polka-storage-provider-client` CLI tool.

## `wallet`

The `wallet` command is a thin wrapper over the [`subkey`](https://docs.substrate.io/reference/command-line-tools/subkey/) utility provided by Polkadot.

More information available on the [`wallet`](./wallet.md) page.

## `info`

The `info` command retrieves information about the storage provider it connects to.

```bash
$ polka-storage-provider-client info --rpc-server-url "http://127.0.0.1:8000"
{
  "start_time": "2024-11-06T11:29:06.058967136Z",
  "address": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
  "seal_proof": "StackedDRG2KiBV1P1",
  "post_proof": "StackedDRGWindow2KiBV1P1"
}
```

## `propose-deal`

The `propose-deal` command sends an *unsigned* deal to the storage provider,
if the storage provider accepts the deal, a CID will be returned,
that CID can then be used to upload a file to the storage provider —
for details on this process, refer to the [File Upload chapter](../../getting-started/demo-file-store.md).

> For the current MVP, the storage provider accepts all valid deals!

```bash
$ DEAL_TO_PROPOSE='{
    "piece_cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "",
    "start_block": 200,
    "end_block": 250,
    "storage_price_per_block": 500,
    "provider_collateral": 1250,
    "state": "Published"
}'
# when we omit the `--rpc-server-address` it defaults to "http://127.0.0.1:8000"
$ polka-storage-provider-client propose-deal "$DEAL_TO_PROPOSE"
bagaaieradsfmawozrmgjwxosarexpg7w7ytoe7xw2c63hv6svdc5hpucqo3a
```

## `sign-deal`

The `sign-deal` commands takes a deal like the one passed to [`propose-deal`](#propose-deal) and signs it using the passed key,
the returned deal can then be used with [`publish-deal`](#publish-deal) to send a deal for publishing.
*This command does not call out to the network.*

```bash
$ DEAL_TO_SIGN='{
    "piece_cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "",
    "start_block": 200,
    "end_block": 250,
    "storage_price_per_block": 500,
    "provider_collateral": 1250,
    "state": "Published"
}'
$ polka-storage-provider-client sign-deal --sr25519-key "//Charlie" "$DEAL_TO_SIGN"
{
  "deal_proposal": {
    "piece_cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "",
    "start_block": 200,
    "end_block": 250,
    "storage_price_per_block": 500,
    "provider_collateral": 1250,
    "state": "Published"
  },
  "client_signature": {
    "Sr25519": "32809cd5b53fa3c2e977f77c4e2189dee230b8773946cf94a704f8af19c578289c11ad256b56146195cfc5d7bb8f670003e4575e133f799f19696495046ed58f"
  }
}
```

## `publish-deal`

The `publish-deal` command effectively publishes the deal, its input is a deal signed using [`sign-deal`](#sign-deal),
and the output is the on-chain deal ID.

```bash
$ SIGNED_DEAL='{
  "deal_proposal": {
    "piece_cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "",
    "start_block": 200,
    "end_block": 250,
    "storage_price_per_block": 500,
    "provider_collateral": 1250,
    "state": "Published"
  },
  "client_signature": {
    "Sr25519": "32809cd5b53fa3c2e977f77c4e2189dee230b8773946cf94a704f8af19c578289c11ad256b56146195cfc5d7bb8f670003e4575e133f799f19696495046ed58f"
  }
}'
$ polka-storage-provider-client publish-deal "$SIGNED_DEAL"
0
```
