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

Generates a [PoRep](../../glossary.md#proofs) parameters which consist of Proving Params (\*.porep.params file) and Verifying Key (\*.porep.vk, \*.porep.vk.scale).
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

Generates a [PoSt](../../glossary.md#proofs) parameters which consist of Proving Params (\*.post.params file) and Verifying Key (\*.post.vk, \*.post.vk.scale).
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

This is a *demo command*, showcasing the ability to generate a PoRep
given the proving parameters so it can later be used to verify proof on-chain.
It uses hardcoded values, which normally would be sourced from the chain i.e:

```rust
let sector_id = 77;
let ticket = [12u8; 32];
let seed = [13u8; 32];
```

```bash
polka-storage-provider-client proofs porep \
    --sr25519-key|--ecdsa-key|--ed25519-key <KEY> \
    --cache-directory <CACHE_DIRECTORY> \
    --proofs-parameters-path <PROVING_PARAMS_FILE> \
    <INPUT_FILE> <INPUT_FILE_PIECE_CID>
```

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

This is a *demo command*, showcasing the ability to generate a PoSt,
given the proving parameters so it can later be used to verify proof on-chain.
It uses hardcoded values, which normally would be sourced from the chain i.e:

```rust
let sector_id = 77;
let randomness = [1u8; 32];
```

```bash
polka-storage-provider-client proofs post
  --sr25519-key|--ecdsa-key|--ed25519-key <KEY> \
  --proof-parameters-path <PROOF_PARAMETERS_PATH> \
  --cache-directory <CACHE_DIRECTORY> \
  <REPLICA_PATH>
  <COMM_R>
```

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