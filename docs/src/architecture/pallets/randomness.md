# Randomness Pallet

## Table of Contents

- [Randomness Pallet](#randomness-pallet)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
  - [Usage](#usage)
  - [Extrinsics](#extrinsics)
  - [Events](#events)
  - [Errors](#errors)
  - [Constants](#constants)

## Overview

The Randomness Pallet provides secure randomness for on-chain operations, primarily for the proof systems used in storage verification.

It captures and stores the VRF (Verifiable Random Function) output from each block author and maintains a history of these values for later use. This randomness is essential for the sealing pipeline's pre-commit and prove commit operations, which are used for generating replicas and proving sectors.

## Usage

This pallet exposes randomness through two main interfaces:

1. `frame_support::traits::Randomness` - A general trait for generating randomness based on a subject
2. `primitives::randomness::AuthorVrfHistory` - A trait for accessing historical randomness values

The randomness is derived from the block author's VRF output combined with a user-supplied subject, providing domain separation for different applications of the same underlying randomness.

## Extrinsics

The pallet does not expose any extrinsics.

## Events

The pallet does not emit any events.

## Errors

The Randomness Pallet actions can fail with the following errors:

- `SeedNotAvailable` - Returned when attempting to access randomness for a block number that is not available in the history.
