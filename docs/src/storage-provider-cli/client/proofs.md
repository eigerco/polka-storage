# `proofs`

The following subcommands are contained under `proofs`.

> These are advanced commands and only useful for demo purposes.
> This functionality is covered in the server by the [pipeline](../../architecture/polka-storage-provider-server.md#sealing-pipeline).

| Name           | Description                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `commp`        | Calculate a piece commitment (CommP) for the provided data stored at the a given path.                                                      |
| `porep-params` | Generates PoRep verifying key and proving parameters for zk-SNARK workflows (prove commit)                                                  |
| `post-params`  | Generates PoSt verifying key and proving parameters for zk-SNARK workflows (submit windowed PoSt)                                           |
| `porep`        | Generates PoRep for a piece file. Takes a piece file (in a CARv2 archive, unpadded), puts it into a sector (temp file), seals and proves it |
| `post`         | Creates a PoSt for a single sector                                                                                                          |

## `commp`

Produces a CommP out of the CARv2 archive and calculates [piece_size](https://spec.filecoin.io/#section-systems.filecoin_files.piece.data-representation) that will be accepted by the network in a [deal](./index.md#propose-deal).
If the file at the path is not a CARv2 archive, it fails.
To create a CARv2 archive, you can use [`mater-cli convert`](../../mater-cli/index.md#convert) command.

### Example

```bash
$ mater-cli convert polkadot.svg
Converted polkadot.svg and saved the CARv2 file at polkadot.car with a CID of bafkreihoxd7eg2domoh2fxqae35t7ihbonyzcdzh5baevxzrzkaakevuvy
$ polka-storage-provider-client proofs commp polkadot.car
{
    "cid": "baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq",
    "size": 2048
}
```

## `porep-params`

Generates a [PoRep](../../glossary.md#proofs) parameters which consist of Proving Params (`*.porep.params` file) and Verifying Key (`*.porep.vk`, `*.porep.vk.scale`).
Proving Parameters are used by the Storage Provider to generate a PoRep and the corresponding Verifying Key is used to [verify proofs on chain](../../architecture/pallets/proofs.md#set_porep_verifying_key) by pallet-proofs and [pallet-storage-provider](../../architecture/pallets/storage-provider.md#prove_commit_sectors).

### Example

```bash
$ polka-storage-provider-client proofs porep-params
Generating params for 2KiB sectors... It can take a couple of minutes ⌛
Generated parameters:
[...]/polka-storage/2KiB.porep.params
[...]/polka-storage/2KiB.porep.vk
[...]/polka-storage/2KiB.porep.vk.scale
```

## `post-params`

Generates a [PoSt](../../glossary.md#proofs) parameters which consist of Proving Params (`*.post.params` file) and Verifying Key (`*.post.vk`, `*.post.vk.scale`).
Proving Parameters are used by the Storage Provider to generate a PoSt and the corresponding Verifying Key is used to [verify proofs on chain](../../architecture/pallets/proofs.md#set_post_verifying_key) by pallet-proofs and [pallet-storage-provider](../../architecture/pallets/storage-provider.md#submit_windowed_post).

### Example

```bash
$ polka-storage-provider-client proofs post-params
Generating PoSt params for 2KiB sectors... It can take a few secs ⌛
Generated parameters:
[...]/polka-storage/2KiB.post.params
[...]/polka-storage/2KiB.post.vk
[...]/polka-storage/2KiB.post.vk.scale
```

## `porep`

Generates a 2KiB sector-size PoRep proof for an input file and its piece commitment.
Creates the sector containing only 1 piece, [seals it](https://spec.filecoin.io/#section-algorithms.pos.porep) by creating a replica and then creates a proof for it.

> This is a _demo command_, showcasing the ability to generate a PoRep
> given the proving parameters so it can later be used to verify proof on-chain.
> It uses hardcoded values, which normally would be sourced from the chain i.e:
>
> ```rust
> let sector_id = 77;
> let ticket = [12u8; 32];
> let seed = [13u8; 32];
> ```

```bash
polka-storage-provider-client proofs porep \
    --sr25519-key|--ecdsa-key|--ed25519-key <KEY> \
    --seal-proof <SEAL_PROOF> \
    --cache-directory <CACHE_DIRECTORY> \
    --proof-parameters-path <PROVING_PARAMS_FILE> \
    --output-path <OUTPUT_PATH> \
    --sector-id <SECTOR_ID> \
    --seal-randomness-height <SEAL_RANDOMNESS_HEIGHT> \
    --pre-commit-block-number <PRE_COMMIT_BLOCK_NUMBER> \
    <INPUT_FILE> <INPUT_FILE_PIECE_CID>
```

<details>
<summary>Click to view the command's arguments description</summary>

```
DEMO COMMAND - Generates PoRep for a piece file.

Takes a piece file (in a CARv2 archive, unpadded), puts it into a sector (temp file), seals and proves it.

When you run the command for the first time on a clean `cache_directory` it will fail, because `rust-fil-proofs` tries to validate cache based on
https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/storage-proofs-porep/parent_cache.json.

When you run the command for the second time, the cache is recreated and there are no verification issues.

Usage: polka-storage-provider-client proofs porep [OPTIONS] --proof-parameters-path <PROOF_PARAMETERS_PATH> --cache-directory <CACHE_DIRECTORY> --sector-id <SECTOR_ID> --seal-randomness-height <SEAL_RANDOMNESS_HEIGHT> --pre-commit-block-number <PRE_COMMIT_BLOCK_NUMBER> <INPUT_PATH> <COMMP>

Arguments:
  <INPUT_PATH>
          Piece file, CARv2 archive created with `mater-cli convert`

  <COMMP>
          CommP of a file, calculated with `commp` command

Options:
      --sr25519-key <SR25519_KEY>
          Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

          See `sp_core::crypto::Pair::from_string_with_seed` for more information.

      --ecdsa-key <ECDSA_KEY>
          ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

          See `sp_core::crypto::Pair::from_string_with_seed` for more information.

      --ed25519-key <ED25519_KEY>
          Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

          See `sp_core::crypto::Pair::from_string_with_seed` for more information.

  -s, --seal-proof <SEAL_PROOF>
          PoRep has multiple variants dependent on the sector size. Parameters are required for each sector size and its corresponding PoRep Params

          [default: 2KiB]
          [possible values: 2KiB, 8MiB, 512MiB, 1GiB]

  -p, --proof-parameters-path <PROOF_PARAMETERS_PATH>
          Path to where parameters to corresponding `seal_proof` are stored

  -c, --cache-directory <CACHE_DIRECTORY>
          Directory where sector data like PersistentAux and TemporaryAux are stored

  -o, --output-path <OUTPUT_PATH>
          Directory where the proof files and the sector will be put. Defaults to the current directory

      --sector-id <SECTOR_ID>
          Sector number

      --seal-randomness-height <SEAL_RANDOMNESS_HEIGHT>
          The height at which we draw the randomness for deriving a sealed cid

      --pre-commit-block-number <PRE_COMMIT_BLOCK_NUMBER>
          Precommit block number

  -h, --help
          Print help (see a summary with '-h')
```

</details>

### Example

```bash
$ mater-cli convert polkadot.svg
Converted polkadot.svg and saved the CARv2 file at polkadot.car with a CID of bafkreihoxd7eg2domoh2fxqae35t7ihbonyzcdzh5baevxzrzkaakevuvy
$ polka-storage-provider-client proofs commp polkadot.car
{
    "cid": "baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq",
    "size": 2048
}
$ polka-storage-provider-client proofs porep-params
Generating params for 2KiB sectors... It can take a couple of minutes ⌛
Generated parameters:
[...]/polka-storage/2KiB.porep.params
[...]/polka-storage/2KiB.porep.vk
[...]/polka-storage/2KiB.porep.vk.scale
$ mkdir -p /tmp/psp-cache
$ polka-storage-provider-client proofs porep --sr25519-key "//Alice" --cache-directory /tmp/psp-cache --proof-parameters-path 2KiB.porep.params polkadot.car baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq
Creating sector...
Precommitting...
2024-11-18T10:48:29.858550Z  INFO filecoin_proofs::api::seal: seal_pre_commit_phase1:start: SectorId(77)
2024-11-18T10:48:29.863782Z  INFO storage_proofs_porep::stacked::vanilla::proof: replicate_phase1
2024-11-18T10:48:29.864120Z  INFO storage_proofs_porep::stacked::vanilla::graph: using parent_cache[64 / 64]
[...]
CommD: Cid(baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq)
CommR: Cid(bagboea4b5abcb7rgo7kuqigb2wjybggbvlmmatmki52by3wov5uwjrjwefxwzxi5)
Wrote proof to [...]/polka-storage/77.sector.proof.porep.scale
```

## `post`

Generates a 2KiB sector-sized PoSt proof.
To be able to create a PoSt proof, first you need to generate a PoRep proof and a replica via `porep` command.

> This is a _demo command_, showcasing the ability to generate a PoSt,
> given the proving parameters so it can later be used to verify proof on-chain.
> It uses hardcoded values, which normally would be sourced from the chain i.e:
>
> ```rust
> let sector_id = 77;
> let randomness = [1u8; 32];
> ```

```bash
polka-storage-provider-client proofs post \
  --sr25519-key|--ecdsa-key|--ed25519-key <KEY> \
  --post-type <POST_TYPE> \
  --proof-parameters-path <PROOF_PARAMETERS_PATH> \
  --cache-directory <CACHE_DIRECTORY> \
  --output-path <OUTPUT_PATH> \
  --sector-number <SECTOR_NUMBER> \
  --challenge-block <CHALLENGE_BLOCK> \
  <REPLICA_PATH> \
  <COMM_R>
```

<details>
<summary>Click to view the command's arguments description</summary>

```
Creates a PoSt for a single sector

Usage: polka-storage-provider-client proofs post [OPTIONS] --proof-parameters-path <PROOF_PARAMETERS_PATH> --cache-directory <CACHE_DIRECTORY> --sector-number <SECTOR_NUMBER> --challenge-block <CHALLENGE_BLOCK> <REPLICA_PATH> <COMM_R>

Arguments:
  <REPLICA_PATH>
          Replica file generated with `porep` command e.g. `77.sector.sealed`

  <COMM_R>
          CID - CommR of a replica (output of `porep` command)

Options:
      --sr25519-key <SR25519_KEY>
          Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

          See `sp_core::crypto::Pair::from_string_with_seed` for more information.

      --ecdsa-key <ECDSA_KEY>
          ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

          See `sp_core::crypto::Pair::from_string_with_seed` for more information.

      --ed25519-key <ED25519_KEY>
          Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.

          See `sp_core::crypto::Pair::from_string_with_seed` for more information.

      --post-type <POST_TYPE>
          PoSt has multiple variants dependant on the sector size. Parameters are required for each sector size and its corresponding PoSt

          [default: 2KiB]
          [possible values: 2KiB, 8MiB, 512MiB, 1GiB]

  -p, --proof-parameters-path <PROOF_PARAMETERS_PATH>
          Path to where parameters to corresponding `post_type` are stored

  -c, --cache-directory <CACHE_DIRECTORY>
          Directory where cache data from `porep` for the `replica_path` sector command has been stored. It must be the same, or else it won't work

  -o, --output-path <OUTPUT_PATH>
          Directory where the PoSt proof will be stored. Defaults to the current directory

      --sector-number <SECTOR_NUMBER>
          Sector Number used in the PoRep command

      --challenge-block <CHALLENGE_BLOCK>
          Block Number at which the randomness should be fetched from. It comes from the [`pallet_storage_provider::DeadlineInfo::challenge`] field

  -h, --help
          Print help (see a summary with '-h')
```

</details>

### Example

```bash
$ polka-storage-provider-client proofs post-params
Generating PoSt params for 2KiB sectors... It can take a few secs ⌛
Generated parameters:
[...]/polka-storage/2KiB.post.params
[...]/polka-storage/2KiB.post.vk
[...]/polka-storage/2KiB.post.vk.scale
$ polka-storage-provider-client proofs post --sr25519-key "//Alice" --cache-directory /tmp/psp-cache --proof-parameters-path 2KiB.post.params 77.sector.sealed bagboea4b5abcb7rgo7kuqigb2wjybggbvlmmatmki52by3wov5uwjrjwefxwzxi5
Loading parameters...
2024-11-18T11:20:26.718119Z  INFO storage_proofs_core::compound_proof: vanilla_proofs:start
2024-11-18T11:20:26.750347Z  INFO storage_proofs_core::compound_proof: vanilla_proofs:finish
2024-11-18T11:20:26.750712Z  INFO storage_proofs_core::compound_proof: snark_proof:start
2024-11-18T11:20:26.750797Z  INFO bellperson::groth16::prover::native: Bellperson 0.26.0 is being used!
2024-11-18T11:20:26.771368Z  INFO bellperson::groth16::prover::native: synthesis time: 20.550334ms
2024-11-18T11:20:26.771385Z  INFO bellperson::groth16::prover::native: starting proof timer
2024-11-18T11:20:26.772676Z  INFO bellperson::gpu::locks: GPU is available for FFT!
2024-11-18T11:20:26.772687Z  INFO bellperson::gpu::locks: BELLPERSON_GPUS_PER_LOCK fallback to single lock mode
Proving...
Wrote proof to [...]/polka-storage/77.sector.proof.post.scale
```
