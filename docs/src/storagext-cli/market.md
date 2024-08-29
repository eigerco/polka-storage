# The `market` command

Under the `market` subcommand you will find the [Market](../pallets/market.md) related extrinsics,
in this chapter, we cover the provided commands and how to use them.

<div class="warning">
If you haven't done so before, you should read the <a href="./index.md"><code>storagext-cli</code> getting started</a> page,
which covers the basic flags necessary to operate the CLI.
</div>

## `add-balance`

The `add-balance` command allows you to add balance to your market account.
It takes a single `AMOUNT` argument, the balance to add to the market account,
the balance will be added to the `free` balance.

### Parameters

| Name     | Description                                  | Type             |
| -------- | -------------------------------------------- | ---------------- |
| `AMOUNT` | The amount to be added to the market balance | Positive integer |

### <a class="header" id="add-balance.example" href="#add-balance.example">Example</a>

Adding 1000000 [Plancks](../glossary.md#planck) to Alice's account.

```bash
storagext-cli --sr25519-key "//Alice" market add-balance 1000000
```

> You can read more about this extrinsic in [_Pallets/Market Pallet/Add Balance_](../pallets/market.md#add_balance).

## `withdraw-balance`

The `withdraw-balance` command allows you to withdraw balance from your market account.
Like [`add-balance`](#add-balance), `withdraw-balance` takes a single `AMOUNT` argument;
note that _only `free` balance can be withdrawn_, like wise,
you can only withdraw a balance amount lesser than or equal to the `free` amount and greater than 0 (\\({free} \ge {amount} \gt 0\\)).

### Parameters

| Name     | Description                                      | Type             |
| -------- | ------------------------------------------------ | ---------------- |
| `AMOUNT` | The amount to be withdrawn to the market balance | Positive integer |

### <a class="header" id="withdraw-balance.example" href="#withdraw-balance.example">Example</a>

Withdrawing 10000 [Plancks](../glossary.md#planck) from Alice's account.

```bash
storagext-cli --sr25519-key "//Alice" market withdraw-balance 10000
```

> You can read more about this extrinsic in [_Pallets/Market Pallet/Withdraw Balance_](../pallets/market.md#withdraw-balance).

## `publish-storage-deals`

The `publish-storage-deals` command allows you to publish storage deals that have been agreed upon off-chain.
The deals are to be submitted by the storage provider, having been previously signed by the client.

<div class="warning">
Since this CLI is currently targetted at testing and demos, it requires the client keypair to sign the deal —
we are aware this is <i>not secure</i> and <i>not realistic</i> in a production scenario (good thing this is a demo)!
</div>

### Parameters

The client keypair can be passed using `--client-<key kind>`, where `<key kind>` is one of the [three supported keys](index.md#getting-started), like the global keys, one is required.

| Name                   | Description               | Type                                                                                                                        |
| ---------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `--client-sr25519-key` | Sr25519 keypair           | String, encoded as hex, BIP-39 or a dev phrase like `//Charlie`                                                             |
| `--client-ecdsa-key`   | ECDSA keypair             | String, encoded as hex, BIP-39 or a dev phrase like `//Charlie`                                                             |
| `--client-ed25519-key` | Ed25519 keypair           | String, encoded as hex, BIP-39 or a dev phrase like `//Charlie`                                                             |
| `DEALS`                | The deals to be published | JSON array. Can be passed as a string, or as a file path prefixed with `@` pointing to the file containing the JSON object. |

The `DEALS` JSON array is composed of objects:

| Name                      | Description                                                         |
| ------------------------- | ------------------------------------------------------------------- |
| `piece_cid`               | Byte encoded CID                                                    |
| `piece_size`              | Size of the piece                                                   |
| `client`                  | SS58 address of the storage client                                  |
| `provider`                | SS58 address of the storage provider                                |
| `label`                   | Arbitrary client chosen label                                       |
| `start_block`             | Block number on which the deal should start                         |
| `end_block`               | Block number on which the deal should end                           |
| `storage_price_per_block` | Price for the storage specified per block[^storage_price_per_block] |
| `provider_collateral`     | Collateral which is slashed if the deal fails                       |
| `state`                   | Deal state. Can only be set to `Published`                          |

### <a class="header" id="publish-storage-deals.example" href="#publish-storage-deals.example">Example</a>

Publishing deals between Alice (the Storage Provider) and Charlie (the client).

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

> You can read more about this extrinsic in [_Pallets/Market Pallet/Publish Storage Deals_](../pallets/market.md#publish_storage_deals).

## `settle-deal-payments`

The `settle-deal-payments` command allows you to settle deal payments,
that is, have the storage provider receive the owed funds from storing data for their clients.
Non-existing deal IDs will be ignored.

Anyone can settle anyone's deals, though there's little incentive to do so — if you're feeling charitable, who are we to stop you?!

### Parameters

| Name       | Description                         |
| ---------- | ----------------------------------- |
| `DEAL_IDS` | The IDs for the deals to be settled |

### <a class="header" id="settle-deal-payments.example" href="#settle-deal-payments.example">Example</a>

Settling deals with the IDs 97, 1010, 1337, 42069:

```bash
storagext-cli --sr25519-key "//Alice" market settle-deal-payments 97 1010 1337 42069
```

> You can read more about this extrinsic in [_Pallets/Market Pallet/Settle Deal Payments_](../pallets/market.md#settle_deal_payments).

## `retrieve-balance`

The `retrieve-balance` command allows you to check the balance of any market account.

### Parameters

| Name | Description |
| --- | --- |
| `ACCOUNT_ID` | The IDs of the account being checked |

### <a class="header" id="settle-deal-payments.example" href="#settle-deal-payments.example">Example</a>

```bash
storagext-cli market retrieve-balance "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY" # Alice's account
```

> This extrinsic IS NOT signed, and as such, does not need to be called using the `--X-key` flag.
