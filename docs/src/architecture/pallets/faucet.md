# Faucet Pallet

## Table of Contents

- [Faucet Pallet](#faucet-pallet)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
  - [Usage](#usage)
  - [Extrinsics](#extrinsics)
    - [`drip`](#drip)
  - [Events](#events)
  - [Errors](#errors)
  - [Constants](#constants)

## Overview

The Faucet Pallet enables users to drip funds into their wallet to start using the polka-storage chain.

<div class="warning">

The faucet pallet only exists on the testnet. Any of the funds dripped do not have any real-world value.

</div>

## Usage

The Faucet Pallet is used to get funds on testnet into an externally generated account.

> Only 1 drip per account is allowed within the configured `FaucetDripDelay` period (which is 1 day on testnet). When trying to drip more often, the transaction will be rejected with a `FaucetUsedRecently` error.

## Extrinsics

### `drip`

The `drip` extrinsic is an [unsigned extrinsic](https://docs.substrate.io/learn/transaction-types/#unsigned-transactions) with no gas fees. This means that any account can get funds, even if their current balance is 0.

| Name      | Description                             | Type                                                                     |
| --------- | --------------------------------------- | ------------------------------------------------------------------------ |
| `account` | The target account to transfer funds to | [SS58 address](https://docs.substrate.io/learn/accounts-addresses-keys/) |

#### <a class="header" id="drip.example" href="#drip.example">Example</a>

```bash
storagext-cli faucet drip 5GpRRVXgPSoKVmUzyinpJPiCjfn98DsuuHgMV2f9s5NCzG19
```

## Events

The Faucet Pallet only emits a single event:

- `Dripped` - Emitted when an account uses the drip function successfully.
  - `who` - [SS58 address](https://docs.substrate.io/learn/accounts-addresses-keys/) of the dripped account.
  - `when` - Block at which the drip occurred.

## Errors

The Faucet Pallet actions can fail with the following errors:

- `FaucetUsedRecently` - Emitted when an account tries to call the drip function more than once within the `FaucetDripDelay` period.

## Constants

The Faucet Pallet has the following constants:

| Name               | Description                                                           | Value              |
| ------------------ | --------------------------------------------------------------------- | ------------------ |
| `FaucetDripAmount` | The amount that is dispensed in [planck](../../glossary.md#planck)'s. | 10_000_000_000_000 |
| `FaucetDripDelay`  | How often an account can use the drip function (1 day on testnet).    | 1 Day              |
