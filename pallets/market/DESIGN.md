# Market Pallet 

## Overview

A [Storage Client][1] finds a [Storage Provider][2] and they negotiate off-chain.
Storage Provider Discovery is handled by [`Storage Provider Pallet`][4], it exposes a function to list all of the Storage Providers withing the system.
Price Negotiation is handled by **libp2p protocol** `Storage Query Protocol` and verified by `Storage Deal Protocol`, it happens off-chain.
The data is transferred between the parties and the Storage Provider adds [collateral][3], locks the fund and _publishes_ the deal on-chain.
The Storage Provider seals the data and _activates_ the deal.

[`Storage Query Protocol` and `Storage Deal Protocol`][5] are implemented by Storage Provider Node and are not the part of Market Pallet implementation.

## Data Structures

```rust
enum DealState<BlockNumber> {
    /// Deal has been negotiated off-chain and is being proposed via `publish_storage_deals`.
    Unpublished,
    /// Deal has been accepted on-chain by both Storage Provider and Storage Client, it's waiting for activation.
    Published,
    /// Deal has been activated
    Active(ActiveDealState<BlockNumber>)
}


/// State only related to the activated deal
/// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/deal.rs#L138>
struct ActiveDealState<BlockNumber> {
    /// Sector in which given piece has been included
    sector_number: SectorNumber,

    /// At which block (time) the deal's sector has been activated.
    sector_start_block: BlockNumber,
    last_updated_block: Option<BlockNumber>,

    /// When the deal was last slashed, can be never.
    slash_block: Option<BlockNumber>
}

/// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/deal.rs#L93>
struct DealProposal<Address, Balance, BlockNumber> {
    piece_cid: Cid,
    piece_size: u64,
    /// Storage Client's Account Id
    client: Address,
    /// Storage Provider's Account Id
    provider: Address,

    /// Arbitrary client chosen label to apply to the deal
    label: String,

    /// Nominal start block. Deal payment is linear between StartBlock and EndBlock,
    /// with total amount StoragePricePerBlock * (EndBlock - StartBlock).
    /// Storage deal must appear in a sealed (proven) sector no later than StartBlock,
    /// otherwise it is invalid.
    start_block: BlockNumber,
    /// When the Deal is supposed to end.
    end_block: BlockNumber,
    /// `Deal` can be terminated early, by `on_sectors_terminate`. 
    /// Before that, a Storage Provider can payout it's earned fees by calling `on_settle_deal_payments`.
    /// `on_settle_deal_payments` must know how much money it can payout, so it's related to the number of blocks (time) it was stored. 
    /// Reference <https://spec.filecoin.io/#section-systems.filecoin_markets.onchain_storage_market.storage_deal_states>
    storage_price_per_block: Balance,

    /// Amount of Balance (DOTs) Storage Provider stakes as Collateral for storing given `piece_cid`
    /// There should be enough Balance added by `add_balance` by Storage Provider to cover it.
    /// When the Deal fails/is terminated to early, this is the amount which get slashed.
    provider_collateral: Balance,
    /// Current [`DealState`].
    /// It goes: `Unpublished` -> `Published` -> `Active`
    state: DealState<BlockNumber>,
}

struct DealId(u64);

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
    - anyone can call this method, the caller is paying for the gas, so usually it's only in Storage Provider interest to do that
9. Storage Provider calls `market.on_sector_terminate(block: BlockNumber, sectors: Vec<SectorNumber>)` to notify market that the sectors no longer exist.
    - if storage was terminated to early, slash the SP, return the funds to the client
    - else, just clean-up data structures used for deals
10. In the meantime, on each block authored, a Hook is executed that checks whether the `Published` deal have been activated. If they were supposed to be activated, but were not, Storage Provider is slashed and client refunded.

[1]: ../../docs/glossary.md#storage-user
[2]: ../../docs/glossary.md#storage-provider
[3]: ../../docs/glossary.md#collateral
[4]: ../storage-provider/DESIGN.md
[5]: https://spec.filecoin.io/#section-systems.filecoin_markets.storage_market.protocols