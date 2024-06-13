# Market Pallet 

## Overview

A [Storage Client][1] finds a [Storage Provider][2] and they negotiate off-chain.
Miner Discovery is handled by [`Storage Provider Pallet`][3], it exposes a function to list all of the Storage Providers withing the system.
Price Negotiation is handled by **libp2p protocol** `Storage Query Protocol` and verified by `Storage Deal Protocol`, it happens off-chain.
The data is transferred between the parties and the Storage Provider adds [collateral][4], locks the fund and _publishes_ the deal on-chain.
The Storage Provider seals the data and _activates_ the deal.

`Storage Query Protocol` and `Storage Deal Protocol` are implemented by Storage Provider Node and are not the part of Market Pallet implementation.

## Data Structures

```rust
enum DealState<BlockNumber> {
    Unpublished,
    Published,
    Active(ActiveDealState<BlockNumber>)
}

struct ActiveDealState<BlockNumber> {
    /// Sector in which given piece has been included
    sector_number: SectorNumber,

    /// In which block it has been included into the sector
    sector_start_block: BlockNumber,
    last_updated_block: Option<BlockNumber>,

    /// When the deal was last slashed, can be never.
    slash_block: Option<BlockNumber>
}

struct DealProposal<Address, Currency, BlockNumber> {
    piece_cid: Cid,
    piece_size: u64,
    verified_deal: bool,
    client: Address,
    provider: Address,

    /// Arbitrary client chosen label to apply to the deal
    label: String,

    /// Nominal start block. Deal payment is linear between StartBlock and EndBlock,
    /// with total amount StoragePricePerBlock * (EndBlock - StartBlock).
    /// Storage deal must appear in a sealed (proven) sector no later than StartBlock,
    /// otherwise it is invalid.
    start_block: BlockNumber,
    end_block: BlockNumber,
    storage_price_per_block: Currency,

    provider_collateral: Currency,
    client_collateral: Currency,
    state: DealState<BlockNumber>,
}

type DealId = u64;

/// Proposals are deals that have been proposed and not yet cleaned up after expiry or termination.
/// They are either 'Published' or 'Active'.
type Proposals = StorageMap<DealId, DealProposal>;

/// Bookkeeping of funds deposited by Market Participants
type BalanceTable<T::Config> = StorageMap<T::AccountId, BalanceEntry<T::Currency>>

struct BalanceEntry<Currency> {
    /// Funds available to be used in the market as Collateral or as Payment for Storage
    /// They can be withdrawn at any time
    deposit: Currency,
    /// Funds locked as Collateral or as Payment for Storage
    /// They cannot be withdrawn unless a sector is terminated
    /// Subject to slashing when a Storage Provider misbehaves
    locked: Currency,
}

/// After Storage Client has successfully negotiated with the Storage Provider, they prepare a DealProposal, 
/// sign it with their signature and send to the Storage Provider.
/// Storage Provider only after successful file transfer and verification of the data, calls an extrinsic `market.publish_storage_deals`.
/// The extrinsic call is signed by the Storage Provider and Storage Client's signature is in the message.
/// Based on that, Market Pallet can verify the signature and lock appropriate funds.
struct ClientDealProposal<Address, Currency, BlockNumber, OffchainSignature> {
    pub proposal: DealProposal,
    pub client_signature: OffchainSignature,
}

/// Used for activation of the deals for a given sector
struct SectorDeal<BlockNumber> {
    sector_number: SectorNumber,
    sector_expiry: BlockNumber,
    deal_ids: Vec<DealId>
}
```

## Market Flow

1. Storage Client and Storage Provider negotiate a deal off-chain.
2. Storage Client calls `market.add_balance(amount: BalanceOf<T>)` to make sure it has enough funds in the market to cover the deal.
    - amount is added to the `Market Pallet Account Id`, the AccountId is derived from PalletId.
3. Storage Provider calls `market.add_balance(amount: BalanceOf<T>)` to make sure it has enough funds to cover the deal collateral.
4. In between now and a call by Storage Provider to `market.publish_storage_deals(deals: Vec<DealProposal>)`, any party can call `market.withdraw_balance(amount: BalanceOf<T>)`.
5. Storage Provider calls `market.publish_storage_deals(deals: Vec<DealProposal>)`
    - funds are locked in BalanceTable
    - deals are now Published, if the Storage Provider does activate them within a timeframe, they're slashed.
6. Storage Provider seals the sector.
7. Storage Provider calls `market.activate_deals(sectors: Vec<SectorDeals>)`.
8. Storage Provider can call `market.settle_deal_payments(deals: Vec<DealId>)` to receive funds periodically, for the storage per blocks elapsed.
    - the gas processing fees are on SP, so they call it as frequently as they want
9. Storage Provider calls `market.on_miners_sector_terminate(block: BlockNumber, sectors: Vec<SectorNumber>)` to notify market that the sectors no longer exist.
    - if storage was terminated to early, slash the SP, return the funds to the client
    - else, just clean-up data structures used for deals
10. In the meantime, on each block authored, a Hook is executed that checks whether the `Published` deal have been activated. If they were supposed to be activated, but were not, Storage Provider is slashed and client refunded.

// TODO:
- parameters for activate deals
- parameters for settl deal payments
- parameters for on miners sector terminate

[1]: ../../docs/glossary.md#storage-user
[2]: ../../docs/glossary.md#storage-provider
[4]: ../storage-provider/DESIGN.md
[3]: ../../docs/glossary.md#collateral