# The Market Pallet

This pallet is part of the polka-storage project. The main purpose of the pallet is tracking funds of the storage market participants and managing storage deals between storage providers and clients.

## Design

[link](./DESIGN.md)

## Market Pallet Interface

### Extrinsics

The Market Pallet provides the following extrinsics (functions):

- `add_balance` - Reserves a given amount of currency for usage in the system.
  - `amount` - The amount that is reserved
- `withdraw_balance` - Withdraws funds from the system.
  - `amount` - The amount that is withdrawn
- `settle_deal_payments` - Settle specified deals between providers and clients.
  - `deal_ids` - List of deal IDs being settled
- `publish_storage_deals` - Publishes list of agreed deals to the chain.
  - `proposal` - Specific deal proposal
    - `piece_cid` - Byte encoded CID
    - `piece_size` - Size of the piece
    - `client` - SS58 address of the storage client
    - `provider` - SS58 address of the storage provider
    - `label` - Arbitrary client chosen label
    - `start_block` - Block number on which the deal might start
    - `end_block` - Block number on which the deal is supposed to end
    - `storage_price_per_block` - Price for the storage specified by block
    - `provider_collateral` - Collateral which is slashed if the deal fails
    - `state` - Deal state. Can only be set to `Published`
  - `client_signature` - Client signature of this specific deal proposal

### Events

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

### Errors

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