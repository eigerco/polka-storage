# Randomness Pallet

## Table of Contents

- [Randomness Pallet](#randomness-pallet)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
  - [Usage](#usage)
  - [Extrinsics](#extrinsics)
  - [Events](#events)
  - [Errors](#errors)
  - [Pallet constants](#pallet-constants)

## Overview

It saves a random seed for each block when its finalized and allows to get this randomness later.
There is a limitation - the randomness is available only after 81st block of the chain, due to randomness predictability earlier.
Currently, the seeds are used for sealing pipeline's pre-commit and prove commit, so for generating a replica and proving a sector.

## Usage

It exposes the interface to get randomness on-chain for a certain block via a trait `primitives_proofs::Randomness`
or chain state query `pallet_randomness:SeedsMap`.
Note that, you can only get a randomness for a `current_block - 1` and dependent on the configuration, the old randomness seed are being removed.

## Extrinsics

The pallet does not expose any extrinsics.

## Events

The pallet does not emit any events.

## Errors

The Randomness Pallet actions can fail with the following errors:

- `SeedNotAvailable` - the seed for the given block number is not available, which means the randomness pallet has not gathered randomness for this block yet.

## Pallet constants

The Storage Provider Pallet has the following constants:

| Name              | Description                                                       | Value   |
| ----------------- | ----------------------------------------------------------------- | ------- |
| `CleanupInterval` | Clean-up interval specified in number of blocks between cleanups. | 1 Day   |
| `SeedAgeLimit`    | The number of blocks after which the seed is cleaned up.          | 30 Days |

