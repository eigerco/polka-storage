# The Market Pallet

This pallet is part of the polka-storage project. The main purpose of the pallet is tracking funds of the storage market participants and managing storage deals between storage providers and clients.

## Extrinsics<a href="../glossary.md#extrinsics"><sup>1</sup></a>

### `add_balance`

Reserves a given amount of currency for usage in the system.

The reserved amount will be considered to be `free` until it is used in a deal,
when it will be moved to `locked` and used to pay for the deal.

| Name     | Description               |
| -------- | ------------------------- |
| `amount` | The amount to be reserved |

#### Example

Adding 10000 [Plancks](../glossary.md#planck) to Alice's account:

```bash
storagext-cli --sr25519-key "//Alice" market add-balance 10000
```

### `withdraw_balance`

Withdraws funds from the system.

The funds will be withdrawn from the `free` balance, meaning that `amount` must be
lower than or equal to `free` and greater than 0 (\\({free} \ge {amount} \gt 0\\)).

| Name     | Description                |
| -------- | -------------------------- |
| `amount` | The amount to be withdrawn |

#### Example

Withdrawing 10000 [Plancks](../glossary.md#planck) from Alice's `free` balance:

```bash
storagext-cli --sr25519-key "//Alice" market withdraw-balance 10000
```

### `settle_deal_payments`

Settle specified deals between providers and clients.

Both clients and providers can call this extrinsic, however,
since the settlement is the mechanism through which the provider gets paid,
there is no reason for a client to call this extrinsic.
Non-existing deal IDs will not raise an error, but rather ignored.

| Name       | Description                        |
| ---------- | ---------------------------------- |
| `deal_ids` | List of the deal IDs to be settled |

#### Example

Settling deal payments for IDs 97, 1010, 1337 and 42069:

```bash
storagext-cli --sr25519-key "//Alice" market settle-deal-payments 97 1010 1337 42069
```

### `publish_storage_deals`

Publishes list of deals to the chain.

This extrinsic _must_ be called by a storage provider.

| Name               | Description                                     |
| ------------------ | ----------------------------------------------- |
| `proposal`         | Specific deal proposal, a JSON object                           |
| `client_signature` | Client signature of this specific deal proposal |

#### Deal Proposal Components

| Name                      | Description                                       |
| ------------------------- | ------------------------------------------------- |
| `piece_cid`               | Byte encoded CID                                  |
| `piece_size`              | Size of the piece                                 |
| `client`                  | SS58 address of the storage client                |
| `provider`                | SS58 address of the storage provider              |
| `label`                   | Arbitrary client chosen label                     |
| `start_block`             | Block number on which the deal might start        |
| `end_block`               | Block number on which the deal is supposed to end |
| `storage_price_per_block` | Price for the storage specified by block          |
| `provider_collateral`     | Collateral which is slashed if the deal fails     |
| `state`                   | Deal state. Can only be set to `Published`        |

See the [original Filecoin specification](https://spec.filecoin.io/#section-systems.filecoin_markets.onchain_storage_market.storage_deal_flow) for details.

#### Example

Using the `storagext-cli` you can publish deals with `//Alice` as the storage provider and `//Charlie` as the client by running the following command:

```bash
storagext-cli --sr25519-key "//Alice" market publish-storage-deals \
  --client-sr25519-key "//Charlie" \
  "@deals.json"
```

Where `deals.json` is a file with contents similar to:

```json
[
    {
        "piece_cid": "bafk2bzacecg3xxc4f2ql2hreiuy767u6r72ekdz54k7luieknboaakhft5rgk",
        "piece_size": 1337,
        "client": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "provider": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "label": "Super Cool (but secret) Plans for a new Polkadot Storage Solution",
        "start_block": 69,
        "end_block": 420,
        "storage_price_per_block": 15,
        "provider_collateral": 2000,
        "state": "Published"
    },
    {
        "piece_cid": "bafybeih5zgcgqor3dv6kfdtv3lshv3yfkfewtx73lhedgihlmvpcmywmua",
        "piece_size": 1143,
        "client": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "provider": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "label": "List of problematic (but flying) Boeing planes",
        "start_block": 1010,
        "end_block": 1997,
        "storage_price_per_block": 1,
        "provider_collateral": 3900,
        "state": "Published"
    }
]
```

<div class="warning">
Notice how the CLI command doesn't take the <code>client_signature</code> parameter,
but rather a keypair that is able to sign it.

We are aware that this is **not secure** however, the system is still under development
and this is **not final** but rather a testing tool.
</div>

## Events

The Market Pallet emits the following events:

- `BalanceAdded` - Indicates that some balance was added as _free_ to the Market Pallet account for the usage in the storage market.
  - `who` - SS58 address of then account which added balance
  - `amount` - Amount added
- `BalanceWithdrawn` - Some balance was transferred (free) from the Market Account to the Participant's account.
  - `who` - SS58 address of then account which had withdrawn balance
  - `amount` - Amount withdrawn
- `DealPublished` - Indicates that a deal was successfully published with `publish_storage_deals`.
  - `deal_id` - Unique deal ID
  - `client` - SS58 address of the storage client
  - `provider` - SS58 address of the storage provider
- `DealActivated` - Deal's state has changed to `Active`.
  - `deal_id` - Unique deal ID
  - `client` - SS58 address of the storage client
  - `provider` - SS58 address of the storage provider
- `DealsSettled` - Published after the `settle_deal_payments` extrinsic is called. Indicates which deals were successfully and unsuccessfully settled.
  - `successful` - List of deal IDs that were settled
  - `unsuccessful` - List of deal IDs with the corresponding errors
- `DealSlashed` - Is emitted when some deal expired
  - `deal_id` - Deal ID that was slashed
- `DealTerminated` - Is emitted it indicates that the deal was voluntarily or involuntarily terminated.
  - `deal_id` - Terminated deal ID
  - `client` - SS58 address of the storage client
  - `provider` - SS58 address of the storage provider

## Errors

The Market Pallet actions can fail with following errors:

- `InsufficientFreeFunds` - Market participant does not have enough free funds.
- `NoProposalsToBePublished` - `publish_storage_deals` was called with empty list of `deals`.
- `ProposalsNotPublishedByStorageProvider` - Is returned when calling `publish_storage_deals` and the deals in a list are not published by the same storage provider.
- `AllProposalsInvalid` - `publish_storage_deals` call was supplied with a list of `deals` which are all invalid.
- `DuplicateDeal` - There is more than one deal with this ID in the Sector.
- `DealNotFound` - Tried to activate a deal which is not in the system.
- `DealActivationError` - Tried to activate a deal, but data is malformed.
  - Invalid specified provider.
  - The deal already expired.
  - Sector containing the deal expires before the deal.
  - Invalid deal state.
  - Deal is not pending.
- `DealsTooLargeToFitIntoSector` - Sum of all of the deals piece sizes for a sector exceeds sector size. The sector size is based on the registered proof type. We currently only support registered `StackedDRG2KiBV1P1` proofs which have 2KiB sector sizes.
- `TooManyDealsPerBlock` - Tried to activate too many deals at a given `start_block`.
- `UnexpectedValidationError` - `publish_storage_deals`'s core logic was invoked with a broken invariant that should be called by `validate_deals`. Report an issue if you receive this error.
- `DealPreconditionFailed` - Due to a programmer bug. Report an issue if you receive this error.
