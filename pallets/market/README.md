# The Market Pallet

This pallet is part of the polka-storage project. The main purpose of the pallet is tracking funds of the storage market participants and managing storage deals between storage providers and clients.

## Design

[link](./DESIGN.md)

## Market Pallet Interface

### Extrinsics

The Market Pallet provides the following extrinsics (functions):

- `add_balance` - Is used by the storage providers and clients to reserve some amount for the storage related actions. The amount tracked is spitted to the free and locked parts. The free funds are locked when the participants interact with the chain.
- `withdraw_balance` - Participants can withdrawal the free funds anytime. That means that those funds are not intended for storage related actions anymore.
- `settle_deal_payments` - When the settlement is executed it calculates the fees earned for the deals and transfers those fees to the storage provider.
- `publish_storage_deals` - Used by the storage provider to publish multiple or a single deal to the chain.

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
- `TooManyDealsPerBlock` - Tried to activate too many deals at a given start_block.
