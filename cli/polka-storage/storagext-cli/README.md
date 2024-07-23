# storagext-cli

CLI wrapper around `storagext`, it aims to provide an ergonomic way to execute the extrinsics made available by the Polka Storage Parachain.

The CLI commands are namespaced according to the pallet you will be interacting with,
as such, if you want to interact with the `market` pallet, you can find it's functions under `storagext-cli market`.

## Global Flags

### Keypair — `--X-key`

Extrinsics are *required* to be signed, as such you need to pass your key.

You can pass it as an hex encoded string, seed key (BIP-39) or use the dev phrases available
(e.g. `//Alice` — remember that these are configured as Sr25519 keypairs by default).

Depending on the type of key you use, you should use a different flag as well:

* `--sr25519-key` for Sr25519 keypairs
* `--ecdsa-key` for ECDSA keypairs
* `--ed25519-key` for Ed25519 keypairs

Example:

```
storagext-cli --sr25519 "//Alice" ...
```

### RPC Address — `--node-rpc`

If you so wish, you can also change the node RPC address, this is achieved through the `--node-rpc` flag. The address can be secure or not (i.e. use TLS).

Secure if, for example, you are running the node behind a reverse proxy (like Nginx) which enables TLS for your connections:

```
storagext-cli --node-rpc wss://172.16.10.10:9944 ...
```

Or insecure if, for example, you are running the node locally, using just the standard setup.

```
storagext-cli --node-rpc ws://0.0.0.0:7331 ...
```


## `market`

The `market` subcommand enables you to interact with the `market` pallet,
this is one of the entrypoints for the parachain as you need to add some balance before you can make use of the parachain features.

### `add-balance`

Add a given amount of MDOT to your free balance, this will enable you to store your files in providers or provide space to others.

```
storagext-cli --sr25519-key <key> market add-balance <amount>
```

### `withdraw-balance`

The dual to `add-balance`, `withdraw-balance` allows you to reclaim back DOT from your free balance.
You cannot reclaim DOT from the locked balance, as it is necessary to pay out for faults, etc.

```
storagext-cli --sr25519-key <key> market withdraw-balance <amount>
```

### `publish-storage-deals`

As a storage provider, you are able to publish storage deals you have done off-chain.

```
storagext-cli --sr25519-key <key> market publish-storage-deals <deals>
```

The command takes `deals` as a JSON array, containing one or more storage deals.

<details>
<summary>Example Storage Deals JSON</summary>
<p>

```json
[
    {
        "piece_cid": "bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy",
        "piece_size": 47000000000,
        "client": "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
        "provider": "5DJiX75PZjvntUMeq7XP8qqJ3Tdg6F2Nybk9So1Z5mWArnG2",
        "label": "737-800 schematics",
        "start_block": 1580889600,
        "end_block": 1721747575,
        "storage_price_per_block": 17144352,
        "provider_collateral": 3735928559,
        "state": "Published"
    },
    {
        "piece_cid": "bafybeih5zgcgqor3dv6kfdtv3lshv3yfkfewtx73lhedgihlmvpcmywmua",
        "piece_size": 269490583,
        "client": "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
        "provider": "5DJiX75PZjvntUMeq7XP8qqJ3Tdg6F2Nybk9So1Z5mWArnG2",
        "label": "Falcon C-00000291",
        "start_block": 1721410062,
        "end_block": 1721747843,
        "storage_price_per_block": 46349,
        "provider_collateral": 3735928559,
        "state": "Published"
    }
]
```

</p>
</details>

However, writing a full JSON file in a single command is cumbersome, to solve that,
you prefix a file path with `@` and use the JSON file location instead:

```
storagext-cli --sr25519-key <key> market publish-storage-deals @important-deals.json
```

### `settle-deal-payments`

As a storage provider, you are entitled to your payment (when you are well behaved),
you can reclaim your payment by calling `settle-deal-payments`. The command takes a
list of IDs for the deals to be processed.

```
storagext-cli --sr25519-key <key> market settle-deal-payments <deal ids>
```
