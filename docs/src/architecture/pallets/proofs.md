# Proofs Pallet

## Table of Contents

- [Overview](#overview)
- [Usage](#usage)
- [Extrinsics](#extrinsics)
  - [`set_porep_verifying_key`](#set_porep_verifying_key)
  - [`set_post_verifying_key`](#set_post_verifying_key)
- [Events](#events)
- [Errors](#errors)

## Overview

The `Proofs Pallet` handles all the logic related to verifying [PoRep](../../glossary.md#porep) and [PoSt](../../glossary.md#post) proofs on-chain.
It's called by [`Storage Provider Pallet`](./storage-provider.md) when verifying proofs during the extrinsics [`prove_commit_sectors`](./storage-provider.md#prove_commit_sectors)
and [`submit_windowed_post`](./storage-provider.md#submit_windowed_post). The Pallet **DOES NOT** expose any extrinsic for proofs verification, it only implements a trait that can be [coupled to other pallets](https://education.web3.foundation/docs/Substrate/section8/pallet-coupling).

To verify the proofs properly it needs to have the verifying keys parameters set for the sector size via [`set_porep_verifying_key`](#set_porep_verifying_key) and [`set_post_verifying_key`](#set_post_verifying_key).

## Usage

This pallet can only be directly used via the trait `primitives_proofs::ProofVerification`. However, for the trait to work and not fail with `Error::MisingPoRepVerifyingKey`/`Error::MissingPoStVerifingKey`, the verifying keys need to be set via extrinsics `set_porep_verifying_key`/`set_post_verifying_key`.

Ideally, users shouldn't worry about it, as it will be set by the governance during a trusted setup procedure and then Storage Providers will download the proof generation parameters. However, in the MVP phase, those keys need to be set with the extrinsics after starting a testnet.

Verifying Keys are set for a Sector Size once and then shared across all proof verifications.
Currently, the network only supports 2KiB sector sizes, so parameters need to be generated and set for it.

## Extrinsics

### `set_porep_verifying_key`

Verifying Key is a set of shared parameters used for zk-SNARK proof verification. It can be generated via [`polka-storage-provider-client proofs porep-params`](../../storage-provider-cli/client/proofs.md#porep-params) command. The verifying key used in the verification must match the proving parameters used in the proof generation.

The extrinsic sets the verifying key received in the SCALE-encoded format and then uses it for all the subsequent verification.
Verifying Key is used to verify every PoRep proof across the network.

| Name            | Description                                                   | Type                                   |
| --------------- | ------------------------------------------------------------- | -------------------------------------- |
| `verifying_key` | shared set of parameters used for zk-SNARK proof verification | SCALE encoded bytes of a Verifying Key |

#### <a class="header" id="set_porep_verifying_key.example" href="#set_porep_verifying_key.example">Example</a>

Setting a verifying key from the [^account] `//Alice` account where proof is stored in the `./2KiB.porep.vk.scale` file.

```bash
storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key 2KiB.vk.scale
```

[^account]: Note that in the MVP every account can set a Verifying Key. It's a risky operation that can halt the entire network, because if verifying key changes, Storage Providers needs to update their generating parameters as well.

### `set_post_verifying_key`

Verifying Key is a set of shared parameters used for zk-SNARK proof verification. It can be generated via [`polka-storage-provider-client proofs post-params`](../../storage-provider-cli/client/proofs.md#post-params) command. The verifying key used in the verification must match proving parameters used in the proof generation.

The extrinsic sets the verifying key received in the SCALE-encoded format and then uses it for all the subsequent verification.
Verifying Key is used to verify every PoSt proof across the network.

| Name            | Description                                                   | Type                                   |
| --------------- | ------------------------------------------------------------- | -------------------------------------- |
| `verifying_key` | shared set of parameters used for zk-SNARK proof verification | SCALE encoded bytes of a Verifying Key |

#### <a class="header" id="set_post_verifying_key.example" href="#set_post_verifying_key.example">Example</a>

Setting a verifying key from the [^account] `//Alice` account where proof is stored in the `./2KiB.post.vk.scale` file.

```bash
storagext-cli --sr25519-key "//Alice" proofs set-post-verifying-key 2KiB.vk.scale
```

[^account]: Note that in the MVP every account can set a Verifying Key. It's a risky operation that can halt the entire network, because if verifying key changes, Storage Providers needs to update their generating parameters as well.

## Events

The Proofs Pallet emits the following events:

- `PoRepVerifyingKeyChanged` - PoRep verifying key has been changed.
  - `who` - SS58 address of the caller.
- `PoStVerifyingKeyChanged` - PoSt verifying key has been changed.
  - `who` - SS58 address of the caller.

## Errors

The Proofs Pallet actions can fail with the following errors:

- `InvalidVerifyingKey` - supplied Verifying Key was not in the valid format and could not be deserialized.
- `InvalidPoRepProof` - PoRep proof could not be verified, it was not created for the given replica window.
- `InvalidPoStProof` - PoSt proof could not be verified, it was not created for the given sector.
- `MissingPoRepVerifyingKey` - tried to verify PoRep proof, but the PoRep verifying key was not set previously with the [`set_porep_verifying_key`](#set_post_verifying_key) extrinsic.
- `MissingPoStVerifyingKey` - tried to verify PoSt proof, but the PoSt verifying key was not set previously with the [`set_post_verifying_key`](#set_post_verifying_key) extrinsic.
- `Conversion` - PoRep/PoSt Proof/VerifyingKey are in an invalid format and cannot be deserialized.

