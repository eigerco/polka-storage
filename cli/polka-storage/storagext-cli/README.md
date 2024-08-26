# storagext-cli

CLI wrapper around `storagext`, it aims to provide an ergonomic way to execute the extrinsics made available by the Polka Storage Parachain.

The CLI commands are namespaced according to the pallet you will be interacting with,
as such, if you want to interact with the `market` pallet, you can find it's functions under `storagext-cli market`.

## Global Flags

> [!NOTE]
> Commands that take in JSON objects as input, are also able to read in files with the provided content.
>
> Instead of passing the JSON object as a parameter, pass the JSON filename prefixed with an `@`.
> For example:
> ```
> storagext-cli --sr25519-key <key> storage-provider pre-commit @pre-commit-sector.json
> ```

### Keypair — `--X-key`

Extrinsics are *required* to be signed, as such you need to pass your key.

You can pass it as an hex encoded string, seed key (BIP-39) or use the dev phrases available
(e.g. `//Alice` — remember that these are configured as Sr25519 keypairs by default).

Depending on the type of key you use, you should use a different flag as well:

* `--sr25519-key` for Sr25519 keypairs
* `--ecdsa-key` for ECDSA keypairs
* `--ed25519-key` for Ed25519 keypairs

Example:

```bash
storagext-cli --sr25519-key "//Alice" ...
```

### RPC Address — `--node-rpc`

If you so wish, you can also change the node RPC address, this is achieved through the `--node-rpc` flag. The address can be secure or not (i.e. use TLS).

Secure if, for example, you are running the node behind a reverse proxy (like Nginx) which enables TLS for your connections:

```bash
storagext-cli --node-rpc wss://172.16.10.10:9944 ...
```

Or insecure if, for example, you are running the node locally, using just the standard setup.

```bash
storagext-cli --node-rpc ws://127.0.0.1:7331 ...
```

## `market`

The `market` subcommand enables you to interact with the `market` pallet,
this is one of the entrypoints for the parachain as you need to add some balance before you can make use of the parachain features.

### `add-balance`

Add a given amount of [Plancks](https://wiki.polkadot.network/docs/learn-DOT#the-planck-unit) to your free balance,
this will enable you to store your files in providers or provide space to others.

```bash
storagext-cli --sr25519-key <key> market add-balance <amount>
```

### `withdraw-balance`

The dual to `add-balance`, `withdraw-balance` allows you to reclaim back DOT from your free balance.
You cannot reclaim DOT from the locked balance, as it is necessary to pay out for faults, etc.

```bash
storagext-cli --sr25519-key <key> market withdraw-balance <amount>
```

### `publish-storage-deals`

As a storage provider, you are able to publish storage deals you have done off-chain.
As this is an experimental CLI, you must provide Client's private key to sign a deal.
Normally, you'd just publish a signed message which you received from a client.

```bash
storagext-cli \
    --sr25519-key <key> \
    market publish-storage-deals \
    --client-sr25519-key <client-key> \
    <deals>
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


### `settle-deal-payments`

As a storage provider, you are entitled to your payment (when you are well behaved),
you can claim your payment by calling`settle-deal-payments`. The command takes a
list of IDs for the deals to be processed.

> [!NOTE]
> The deal ID list is separated by spaces, for example:
> ```
> settle_deal_payments 1203 1243 1254
> ```

```bash
storagext-cli --sr25519-key <key> market settle-deal-payments <deal ids>
```


## `storage-provider`

The `storage-provider` subcommand enables you to interact with the `storage-provider` pallet.

### `register`

You need to register as a `Storage Provider` to be able to deal with the clients and perform any storage provider duties.

```bash
storagext-cli --sr25519-key <key> storage-provider register <peer_id>
```

### `pre-commit`

Storage Provider must pre-commit a sector with deals that have been published by `market publish-storage-deals`, so it can later be proven.
If the deals are not pre-commited in any sector and then proven, they'll be slashed.
Deals in the sector are validated, so without calling `publish-storage-deals` it's not possible to execute this function.
`seal-proof` must match the `post-proof` used in `register`.

```bash
storagext-cli --sr25519-key <key> storage-provider pre-commit <pre-commit-sector>
```

This command takes `pre-commit-sector` as JSON Object.

<details>
<summary>Example Pre-commit Sector JSON</summary>
<p>

```json
{
    "sector_number": 1,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 100,
    "unsealed_cid": "bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy",
    "seal_proof": "StackedDRG2KiBV1P1"
}
```

</p>
</details>

### `prove-commit`

Storage Provider must prove commit a sector which has been pre-commited.
If the sector is not proven, deal won't become `Active` and will be **slashed**.

```bash
storagext-cli --sr25519-key <key> storage-provider prove-commit <prove-commit-sector>
```

This command takes a `prove-commit-sector` JSON object, the `proof` must be a valid hex-string.
Proof is accepted if it is any valid hex string of length >= 1.

<details>
<summary>Example Prove Commit Sector JSON</summary>
<p>

```json
{
    "sector_number": 1,
    "proof": "1230deadbeef"
}
```

</p>
</details>

### `submit-windowed-post`

Submit a window [Proof-of-Spacetime](https://spec.filecoin.io/#section-algorithms.pos.post)
to prove the storage provider is still storing the client data.

```bash
storagext-cli --sr25519-key <key> storage-provider submit-windowed-post <window-proof>
```

The command takes a JSON object with four fields — `deadline` a number, `partition` a number,
`chain_commit_block` a block number and `proof` which is another JSON object with fields —
`post_proof` which can either be `2KiB` or `StackedDRGWindow2KiBV1P1` (both amounting to the same value)
and `proof_bytes` which expectes a valid hex string.

<details>
<summary>Example Submit Windowed Proof-of-Spacetime JSON</summary>
<p>

```json
{
    "deadline": 10,
    "partitions": [10],
    "chain_commit_block": 1,
    "proof": {
        "post_proof": "2KiB",
        "proof_bytes": "07482439"
    }
}
```

</p>
</details>

### `declare-faults`

Declare [faulty sectors](https://spec.filecoin.io/#section-systems.filecoin_mining.sector.lifecycle) to avoid penalties for not submitting [Window PoSt](../../../docs/glossary.md#post) at the required time.

```bash
storagext-cli --sr25519-key <key> storage-provider declare-faults <faults>
```

The command takes a list of JSON objects with the fields — `deadline` a number, `partition` a number and `sectors` an array of numbers.
The `deadline` parameter specificies the deadline where to find the respective `partition` and `sectors`.

<details>
<summary>Example Declaration of Faults JSON</summary>
<p>

```json
[
    {
        "deadline": 0,
        "partitions": 0,
        "sectors": [
            0
        ]
    }
]
```

</p>
</details>

### `declare-faults-recovered`

Declare [recovered faulty sectors](https://spec.filecoin.io/#section-systems.filecoin_mining.sector.lifecycle) to avoid penalties over sectors that have been recovered. Note that a sector is only considered to be "fully-healed" (i.e. not suffer any penalties) after a new proof has been submitted.

```bash
storagext-cli --sr25519-key <key> storage-provider declare-faults-recovered <recoveries>
```

The command takes a list of JSON objects with the fields — `deadline` a number, `partition` a number and `sectors` an array of numbers.
The `deadline` parameter specificies the deadline where to find the respective `partition` and `sectors`.

<details>
<summary>Example Declaration of Recovered Faults JSON</summary>
<p>

```json
[
    {
        "deadline": 0,
        "partition": 0,
        "sectors": [
            0
        ]
    }
]
```

</p>
</details>
