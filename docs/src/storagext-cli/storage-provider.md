# The `storage-provider` command

Under the `storage-provider` subcommand [Storage Provider](../architecture/pallets/storage-provider.md) related extrinsics are available.
This chapter covers the provided commands and how to use them.

<div class="warning">
The <a href="./index.md"><code>storagext-cli</code> getting started</a> page covers the basic flags necessary to operate the CLI and should be read first.
</div>

### `register`

The `register` command registers as a storage provider. Before a user can start providing storage, they need to register to be able to deal with the clients and perform any storage provider duties.

### Parameters

| Name         | Description                                                                | Type   |
| ------------ | -------------------------------------------------------------------------- | ------ |
| `PEER_ID`    | The peer ID under which the registered provider will be tracked            | String |
| `POST_PROOF` | The proof type that the provider will use to prove storage (Default: 2KiB) | String |

### <a class="header" id="register.example" href="#register.example">Example</a>

Registering the provider with the specific `peer_id`

```bash
storagext-cli --sr25519-key <key> storage-provider register <peer_id>
```

> More information about the `register` extrinsic is available in [_Pallets/Storage Provider/Register_](../architecture/pallets/storage-provider.md#register_storage_provider).

### `pre-commit`

The `pre-commit` command [pre-commits](../architecture/pallets/storage-provider.md#pre_commit_sectors) a sector with deals that have been published by `market publish-storage-deals`. The pre-committed sector has to be proven or the deals will not activate and will be slashed.

### Parameters

| Name                 | Description                     | Type                                                                                                                         |
| -------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `PRE_COMMIT_SECTORS` | The sector we are committing to | JSON object. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON object. |

The `PRE_COMMIT_SECTORS` JSON object has the following structure:

| Name                     | Description                              |
| ------------------------ | ---------------------------------------- |
| `sector_number`          | Sector number                            |
| `sealed_cid`             | Byte encoded CID                         |
| `deal_ids`               | List of deal IDs                         |
| `expiration`             | Sector expiration                        |
| `unsealed_cid`           | Byte encoded CID                         |
| `seal_proof`             | Sector seal proof                        |
| `seal_randomness_height` | The block number used in the PoRep proof |

### <a class="header" id="pre-commit.example" href="#pre-commit.example">Example</a>

Pre-commits a sector with specified deals.

```bash
storagext-cli --sr25519-key <key> storage-provider pre-commit \
    "@pre-commit-sector.json"
```

Where `pre-commit-sector.json` is a file with contents similar to:

```json
[
  {
    "sector_number": 0,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 100,
    "unsealed_cid": "bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy",
    "seal_proof": "StackedDRG2KiBV1P1",
    "seal_randomness_height": 85
  }
]
```

> More information about the `pre_commit` extrinsic is available in [_Pallets/Storage Provider/Pre-commit sector_](../architecture/pallets/storage-provider.md#pre_commit_sectors).

### `prove-commit`

The `prove-commit` command proves a sector commitment. Currently, any proof that is a hex encoded string of length >= 1 is accepted.
After the sector is proven, the deals will become `Active`.

### Parameters

| Name                   | Description               | Type                                                                                                                         |
| ---------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `PROVE_COMMIT_SECTORS` | The sector we are proving | JSON object. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON object. |

The `PROVE_COMMIT_SECTORS` JSON object has the following structure:

| Name            | Description       |
| --------------- | ----------------- |
| `sector_number` | Sector number     |
| `proof`         | Hex encoded proof |

### <a class="header" id="prove-commit.example" href="#prove-commit.example">Example</a>

Proves a sector commitment.

```bash
storagext-cli --sr25519-key <key> storage-provider prove-commit \
    "@prove-commit-sector.json"
```

Where `prove-commit-sector.json` is a file with contents similar to:

```json
[
  {
    "sector_number": 0,
    "proof": "beef"
  }
]
```

> More information about `prove_commit` extrinsic is available in [_Pallets/Storage Provider/Prove-commit sector_](../architecture/pallets/storage-provider.md#prove_commit_sectors).

### `submit-windowed-post`

The `submit-windowed-post` command submits a windowed PoSt proof. The post proof needs to be periodically submitted to prove that some sector is still stored. Sectors are proven in batches called partitions.

### Parameters

| Name            | Description                  | Type                                                                                                                         |
| --------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `WINDOWED_POST` | The proof for some partition | JSON object. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON object. |

The `WINDOWED_POST` JSON object has the following structure:

| Name        | Description  |
| ----------- | ------------ |
| `deadline`  | Deadline ID  |
| `partition` | Partition ID |
| `PROOF`     | JSON object  |

The `PROOF` JSON object has the following structure:

| Name          | Description                                       |
| ------------- | ------------------------------------------------- |
| `post_proof`  | Proof type ("2KiB" or "StackedDRGWindow2KiBV1P1") |
| `proof_bytes` | Hex encoded proof                                 |

### <a class="header" id="submit-windowed-post.example" href="#submit-windowed-post.example">Example</a>

Proves partitions in a specific deadline.

```bash
storagext-cli --sr25519-key <key> storage-provider submit-windowed-post \
    "@window-proof.json"
```

Where `window-proof.json` is a file with contents similar to:

```json
{
  "deadline": 0,
  "partitions": [0],
  "proof": {
    "post_proof": "2KiB",
    "proof_bytes": "07482439"
  }
}
```

> More information about the `submit_windowed_post` extrinsic is available in [_Pallets/Storage Provider/Submit Windowed Post_](../architecture/pallets/storage-provider.md#submit_windowed_post).

### `declare-faults`

The `declare-faults` command declares faulty sectors. This is required to avoid penalties for not submitting [Window PoSt](../glossary.md#post) at the required time.

### Parameters

| Name     | Description             | Type                                                                                                                       |
| -------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `FAULTS` | List of declared faults | JSON array. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON array. |

The `FAULTS` JSON object has the following structure:

| Name        | Description        |
| ----------- | ------------------ |
| `deadline`  | Deadline ID        |
| `partition` | Partition ID       |
| `sectors`   | Faulty sectors IDs |

### <a class="header" id="declare-faults.example" href="#declare-faults.example">Example</a>

Declares a list of faulty sectors in a specific deadline and partition.

```bash
storagext-cli --sr25519-key <key> storage-provider declare-faults \
    "@faults.json"
```

Where `faults.json` is a file with contents similar to:

```json
[
  {
    "deadline": 0,
    "partition": 0,
    "sectors": [0]
  }
]
```

> More information about the `declare_faults` extrinsic is available in [_Pallets/Storage Provider/Declare Faults_](../architecture/pallets/storage-provider.md#declare_faults).

### `declare-faults-recovered`

The `declare-faults-recovered` command declares recovered faulty sectors.

### Parameters

| Name         | Description                 | Type                                                                                                                       |
| ------------ | --------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `RECOVERIES` | List of declared recoveries | JSON array. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON array. |

The `RECOVERIES` JSON object has the following structure:

| Name        | Description        |
| ----------- | ------------------ |
| `deadline`  | Deadline ID        |
| `partition` | Partition ID       |
| `sectors`   | Faulty sectors IDs |

### <a class="header" id="declare-faults-recovered.example" href="#declare-faults-recovered.example">Example</a>

Declares a list of sectors as recovered in a specific deadline and partition.

```bash
storagext-cli --sr25519-key <key> storage-provider declare-faults-recovered \
    "@recoveries.json"
```

Where `recoveries.json` is a file with contents similar to:

```json
[
  {
    "deadline": 0,
    "partition": 0,
    "sectors": [0]
  }
]
```

> More information about the `declare_faults_recovered` extrinsic is available in [_Pallets/Storage Provider/Declare Faults Recovered_](../architecture/pallets/storage-provider.md#declare_faults_recovered).

### `terminate-sectors`

The `terminate-sectors` command terminates sectors and fully removes them.

### Parameters

| Name           | Description                   | Type                                                                                                                       |
| -------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `TERMINATIONS` | List of declared TERMINATIONS | JSON array. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON array. |

The `RECOVERIES` JSON object has the following structure:

| Name        | Description                     |
| ----------- | ------------------------------- |
| `deadline`  | Deadline ID                     |
| `partition` | Partition ID                    |
| `sectors`   | IDs of sectors to be terminated |

### <a class="header" id="terminate-sectors.example" href="#terminate-sectors.example">Example</a>

Declares a list of sectors as recovered in a specific deadline and partition.

```bash
storagext-cli --sr25519-key <key> storage-provider terminate-sectors \
    "@terminations.json"
```

Where `terminations.json` is a file with contents similar to:

```json
[
  {
    "deadline": 0,
    "partition": 0,
    "sectors": [0]
  }
]
```

> More information about the `terminate_sectors` extrinsic is available in [_Pallets/Storage Provider/Terminate Sectors_](../architecture/pallets/storage-provider.md#terminate_sectors).

### `retrieve-storage-providers`

The `retrieve-storage-providers` command retrieves all registered storage providers.

### <a class="header" id="retrieve-storage-providers.example" href="#retrieve-storage-providers.example">Example</a>

Retrieving all registered storage providers

```bash
storagext-cli storage-provider retrieve-storage-providers
```

> This command **is not signed**, and does not need to be called using any of the `--X-key` flags.
