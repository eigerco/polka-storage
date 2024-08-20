# The Market Pallet

## Design

[link](./DESIGN.md)

## Market Pallet Interface

### Extrinsics

The Market Pallet provides the following extrinsics (functions):

- **add_balance** - Is used by the storage providers and clients to reserve some amount for the storage related actions. The amount tracked is spitted to the free and locked parts. The free funds are locked when the participants interact with the chain.
- **withdraw_balance** - Participants can withdrawal the free funds anytime. That means that those funds are not intended for storage related actions anymore.
- **settle_deal_payments** - When the settlement is executed it calculates the fees earned for the deals and transfers those fees to the storage provider.
- **publish_storage_deals** - Used by the storage provider to publish multiple or a single deal to the chain.

### Events

The Market Pallet emits the following events:

- **BalanceAdded** - Indicates that some balance was reserved for the usage in the storage system.
- **BalanceWithdrawn** - Some balance was unreserved.
- **DealPublished** - Indicates that the deal was successfully published
- **DealActivated** - Published for the deals when they get activated.
- **DealsSettled** - Published after the `publish_storage_deals` extrinsic is called. Indicates which deals were successfully and unsuccessfully settled.
- **DealSlashed** - Is emitted when some deal expired
- **DealTerminated** - If emitted it indicates that the deal was voluntarily or involuntarily terminated.
