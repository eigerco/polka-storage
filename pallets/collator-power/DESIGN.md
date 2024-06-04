# Overall Power Pallet Flow

## Glossary
- **extrinsic** - state transition function on a pallet, essentially a signed transaction which requires an account with some tokens to be executed as it costs fees.
- [Storage Provider][5] - is running a full node and as well off-chain operations and provides storage to the blockchain clients. 
- **collateral** - amount of tokens staked by a miner (via `SPP`) to be able to provide storage services 
- **PoS** - [Proof of Storage][3]
- **PoSt** - [Proof of Space-Time][6]
- `SPP` - Storage Provider Pallet
- `CPP` - Collator Power Pallet
- `CSP` - Collator Selection Pallet
- `CRP` - Collator Reward Pallet
- **session** - a [session][4] is a period of time that has a constant set of validators. 

## Overview

**Collators** are entities selected to produce state transition proofs which are then finalized by relay chain's **validators**.
They aggregate parachain transactions into **parachain block candidates*.
To participate in **block candidate production**, a **Collator** needs to stake some **tokens**.
Proportionally to the amount of **tokens**, a **Collator** has a higher chance to be selected for the **block candidate production**.  
**Collator** can stake his own tokens or a **Storage Provider** can delegate his tokens to the **Collator**.
**Storage Provider** by doing that can earn some tokens, when the **Collator** he delegated his tokens on is chosen for the production.
When a **Collator** is slashed, **Storage Provider** that staked their tokens on them is slashed accordingly. 

**Storage Providers** do not need to stake any tokens on Collator to support their storage resources, it's optional.
When **Storage Providers** misbehave e.g. fail to deliver some proof, they're being slashed from the collateral they pledged when for example:
- securing a new deal with a customer,
- adding storage capacity (which requires pledging).

This pallet works as a proxy between `SPP` and `CSP` to make collator choices.
It stores how much power was delegated by **Miners** to **Collators**.
Both `SPP` and `CSP` are [tightly coupled][2] to this pallet.

## Data Structures

```rust
/// Store of Collators and their metadata
collators: BoundedBTreeMap<CollatorId, StoragePower, ConstU32<100>>
/// List of available Storaged Providers
/// Used as an allowlist for who can stake on a Collator
storage_providers: BoundedBTreeSet<StorageProviderId>

struct CollatorInfo<Collator, StorageProvider, Power> {
    /// Identifier of a Collator
    who: Collator,
    /// Reserved deposit of a Collator
    deposit: Power,
    /// Delegated deposits from Storage Providers to Collators
    delegated_deposit: Map<StorageProvider, Power>
}
```

## Use Cases

### Storage Provider Registration

We need to identify storage providers somehow. 
Calling a `Storage Provider Pallet` would create a circular dependency.
The `SPP` will call the registration function to let the `CPP` now, that a **Storage Provider**
is allowed to stake Power (tokens) on a **Collator**.

#### Assumptions
- `register_storage_provider(storage_provider: T::StorageProviderId)` is a **plain function**, it's called by `Storage Provider Pallet` when a new Storage Provider is registered, we trust the caller. It can only be called from `SPP` via [tight coupling][2].

#### Flow:
1. `SPP` calls `register_storage_provider(storage_provider: T::StorageProviderId)` 
2. `CPP` adds a `storage provider` to the `TreeSet` keeping the list of registered providers

### Collator Registration

#### Assumptions

- **Collator** can register on its own by calling an extrinsic `register_as_collator()`.
- It requires a certain minimum amount of **collateral** (a bond) to be locked, to become a **collator**.
- After you registered as a **collator**, you can update your bond and lock even more **collateral**.

#### Flow

1. A node in the network calls `CPP.register_as_collator(origin: T::CollatorId)`
2. `CPP` verifies whether a account that originated the transaction has a minimum amount of **collateral** to be deposited.
3. `CPP` reserves (locks) deposited balance of the account, through `ReservableCurrency`
3. `CPP` adds `CollatorId` to the `Map<Collator, CollatorInfo>` with the `deposit` equal to the minimum **bond**.

### Adding more Collator Power as a Collator

#### Assumptions

- `CPP.update_bond` is an **extrinsic**, which is called by a **Collator**.
- You cannot update bond on a *Collator* that has not been registered before with `CPP.register_as_collator`
- `CPP.update_bond` can reduce as well as increase deposit, hence the Power

#### Flow

1. **Collator** calls `CPP.update_bond(collator: T::CollatorId, new_deposit: BalanceOf<T>)` 
2. In the next **session**, the saved Power is picked up by `CSP`, by calling `CPP.get_collator_power(collator: T::CollatorId) -> T::StoragePower`. 

### Delegating power to a Collator as a Storage Provider

#### Assumptions

- `update_storage_provider_bond()` is an **extrinsic** that can be called by **Storage Providers** 
- **Storage Provider** is present in the `storage_providers` set -  has been registered with `CPP.register_storage_provider`.
- **Collator** has been registerd in `collators` TreeMap

#### Flow

1. **Storage Provider** calls `CPP.update_storage_provider_bond(storage_provider: T::StorageProviderId, collator: T:CollatorId, new_deposit: BalanceOf<T>)`
2. In the next **session**, the saved Power is picked up by `CSP`, by calling `CPP.get_collator_power(collator: T::CollatorId) -> T::StoragePower`. 

### Slashing 

<!-- TODO(@th7nder,04/06/2024) -->
<!-- slashing flow and when, who calls what, why are those extrinsics -->

[1]: https://github.com/filecoin-project/lotus/blob/9851d35a3811e5339560fb706926bf63a846edae/cmd/lotus-miner/init.go#L638
[2]: https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_pallet_coupling/index.html#tight-coupling-pallets
[3]: https://spec.filecoin.io/#section-algorithms.pos
[4]: https://paritytech.github.io/polkadot-sdk/master/pallet_session/index.html
[5]: https://github.com/eigerco/polka-disk/blob/main/doc/research/lotus/lotus-overview.md#Roles
[6]: https://spec.filecoin.io/#section-algorithms.pos.post