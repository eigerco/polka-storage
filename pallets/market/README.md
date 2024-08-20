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

  - `deal_ids` - List of deal ids being settled

- `publish_storage_deals` - Publishes list of agreed deals to the chain.

  - `proposal` - Specific deal proposal
    - `piece_cid` - Byte encoded cid
    - `piece_size` - Size of the piece
    - `client` - Account of the storage client
    - `provider` - Account of the storage provider
    - `label` - Arbitrary client chosen label
    - `start_block` - Block number on which the deal might start
    - `end_block` - Block number on which the deal is supposed to end
    - `storage_price_per_block` - Price for the storage specified by block
    - `provider_collateral` - Collateral which is slashed if the deal fails
    - `state` - Deal state. Can only be set to `Published`
  - `client_signature` - Client signature of this specific deal proposal

### Events

The Market Pallet emits the following events:

- `BalanceAdded` - Indicates that some balance was reserved for the usage in the storage system.
- `BalanceWithdrawn` - Some balance was unreserved.
- `DealPublished` - Indicates that the deal was successfully published
- `DealActivated` - Published for the deals when they get activated.
- `DealsSettled` - Published after the `publish_storage_deals` extrinsic is called. Indicates which deals were successfully and unsuccessfully settled.
- `DealSlashed` - Is emitted when some deal expired
- `DealTerminated` - If emitted it indicates that the deal was voluntarily or involuntarily terminated.

### Errors

The Market Pallet actions can fail with following errors:

- `InsufficientFreeFunds` - Market participant does not have enough free funds.
- `NoProposalsToBePublished` - `publish_storage_deals` was called with empty `deals` array.
- `ProposalsNotPublishedByStorageProvider` - `publish_storage_deals` must be called by Storage Providers and it's a Provider of all of the deals.
- `AllProposalsInvalid` - `publish_storage_deals` call was supplied with `deals` which are all invalid.
- `UnexpectedValidationError` - `publish_storage_deals`'s core logic was invoked with a broken invariant that should be called by `validate_deals`.
- `DuplicateDeal` - There is more than 1 deal of this ID in the Sector.
- `DealPreconditionFailed` - Due to a programmer bug, bounds on Bounded data structures were incorrect so couldn't insert into them.
- `DealNotFound` - Tried to activate a deal which is not in the system.
- `DealActivationError` - Tried to activate a deal, but data doesn't make sense. Details are in the logs.
- `DealsTooLargeToFitIntoSector` - Sum of all of the deals piece sizes for a sector exceeds sector size.
- `TooManyDealsPerBlock` - Tried to activate too many deals at a given `start_block`.
