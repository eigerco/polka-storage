# Overall Power Pallet Flow

## Glossary
- **extrinsic** - state transition function on a pallet, essentially a signed transaction which requires an account with some tokens to be executed as it costs fees.
- **Miner** - [Storage Provider][5]
- **collateral** - amount of tokens staked by a miner (via `SPP`) to be able to provide storage services 
- **PoS** - [Proof of Storage][3]
- **PoSt** - [Proof of Space-Time][6]
- `SPP` - Storage Provider Pallet
- `CPP` - Collator Power Pallet
- `CSP` - Collator Selection Pallet
- **session** - a [session][4] is a period of time that has a constant set of validators. 

## Overview

**Collators** are entities selected to produce state transition proofs which are then finalized by relay chain's **validators**.
They aggregate parachain transactions into **parachain block candidates*.
To participate in **block candidate production**, a **Collator** needs to:
- stake a ***certain*** (yet to be determined) amount of tokens 
- be backed by **Miners'** **Storage Power**.
Collators' staking is a requirement for participation in the process, the actual selection is based on **Storage Power**.

The collators are selected based on **Storage Power** by `CSP` in an randonmized auction algorithm.
The more **Storage Power** has been staked on a **Collator** by the **Miner**, the more likely their chances to be selected for block production.

This pallet works as a proxy between `SPP` and `CSP` to make collator choices.
It stores how much **Storage Power** a **Miner** has and how much was delegated by **Miners** to **Collators**.
Both `SPP` and `CSP` are [tightly coupled][2] to this pallet.

Trade Offs [?]:

**Collators** are separately tracked by `CSP` and this pallet gives back the `staked_power` to a **Miner** when a **Collator** disappears.
This is an intentional design decision, this could be also tracked in this pallet, however I do think it'd make this Pallet too complex.
As Collators also need to be staked and require their own registration logic.

## Data Structures

```rust
struct MinerClaim {
    /// Indicates how much power a Miner has
    raw_bytes_power: T::StoragePower;
    staked_power: Map<T::CollatorId, T::StoragePower>
}
```

## Use Cases

### Registration

#### Useful links
- [Creating Storage Miner in Lotus][1]

#### Assumptions
- `create_miner(miner: T::MinerId)` is an **extrinsic**. It's called by `Storage Provider Node` on a bootstrap. We can trust it, as `T::MinerId` is essentialy an account ID. The transaction needs to be signed, for it to be signed it needs to come from an account. For it to come from an account, the account has to have an **existential deposit** and it costs money. That's how it's DOS-resistant.

#### Flow:
1. **Miner** calls `create_miner(miner: T::MinerId)` 
2. `CPP` initializes `Map<MinerId, MinerClaim>`.

### Manipulating Power 

#### Assumptions
- `SPP` only calls `CPP.update_miner_power` function after:
    * Miner has been registered in `Collator Power` via `create_miner` function call
    * Miner has submitted `PreCommit` sector with a certain (calculated by `SPP`) amount **Collateral** required
    * Miner has proven sectors with **PoS** via `ProveCommit` of `SPP`.
- `update_miner_power` is ***NOT** an **extrinsic**. It can only be called from `SPP` via [tight coupling][2].
    - The reason is that we can't trust that a **Miner** will call extrinsic and update it on their own. `SPP` logic will perform those updates, e.g: after (not)receiving **PoSt**, receiving **pledge collaterals**.

#### Flow
1. `SPP` calls `CPP.update_miner_power(miner: T::MinerId, deltaStorageBytes: T::StoragePower)`
2. If Storage Power was decresed: `CPP` decreases all delegated power (to **Collators**)
    - essentially means 'Slashing Miner and the Power they delegated'
3. If Storage Power was increased: `CPP` does nothing.
4. `CPP` performs bookeeping, updating `MinerClaim`

### Delegating Power

#### Assumptions
- It's an **extrinsic**, can be called by a **Miner**.

#### Flow
1. **Miner** calls `CPP.delegate_power(miner: T::MinerId, collator: T::CollatorId, amount: T::StoragePower)`
2. `CPP` saves delegated **Storage Power** in **Miner's** claims (`staked_power`).
3. In the next **session**, the saved Power is picked up by `CSP`, by calling `CPP.get_collator_power(collator: T::CollatorId) -> T::StoragePower`. 

#### Slashing Collator (?)

TODO: I don't have this piece of the puzzle yet. I mean... What happens if a **Miner** staked some power on a **Collator** and it misbehaved? Do we **slash** the **Miner's** staked tokens/or storage power, and if so, how?

[1]: https://github.com/filecoin-project/lotus/blob/9851d35a3811e5339560fb706926bf63a846edae/cmd/lotus-miner/init.go#L638
[2]: https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_pallet_coupling/index.html#tight-coupling-pallets
[3]: https://spec.filecoin.io/#section-algorithms.pos
[4]: https://paritytech.github.io/polkadot-sdk/master/pallet_session/index.html
[5]: https://github.com/eigerco/polka-disk/blob/main/doc/research/lotus/lotus-overview.md#Roles
[6]: https://spec.filecoin.io/#section-algorithms.pos.post