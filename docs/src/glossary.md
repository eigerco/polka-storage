# Glossary and Anti-Glossary

This document provides definitions and explanations for terms used throughout the project, as well as a list of terms
that should not be used.

## Table of Contents

- [Glossary](#glossary)
    - [Actor](#actor)
    - [Bond](#bond)
    - [Collateral](#collateral)
    - [Collator](#collator)
    - [Committed Capacity](#committed-capacity)
    - [Crowdloan](#crowdloan)
    - [Full Node](#full-node)
    - [Invulnerable](#invulnerable)
    - [Node](#node)
    - [Parachain](#parachain)
    - [Polkadot](#polkadot)
    - [Proofs](#proofs)
        - [Proof-of-Replication (PoRep)](#porep)
        - [Proof-of-Spacetime (PoSt)](#post)
    - [Relay Chain](#relay-chain)
    - [Session](#session)
    - [Slashing](#slashing)
    - [Slot Auction](#slot-auction)
    - [Staking](#staking)
        - [Nominators](#nominators)
        - [Validators](#validators)
    - [Storage Provider](#storage-provider)
    - [Storage User](#storage-user)
    - [System Parachain](#system-parachain)
- [Anti-Glossary](#anti-glossary)
    - [Miner](#term-to-avoid-miner)
    - [Pledge](#term-to-avoid-pledge)

## Glossary

This section lists terms used throughout the project.

### Actor

In [Filecoin](https://spec.filecoin.io/#section-glossary.filecoin),
an [actor](https://spec.filecoin.io/#section-glossary.actor) is an on-chain object with its own state and set of
methods. [Actors](https://spec.filecoin.io/#section-glossary.actor) define how
the [Filecoin](https://spec.filecoin.io/#section-glossary.filecoin) network manages and updates global state.

### Bond

This term used in:

- Parachain [Slot Auction](#slot-auction). To bid in an auction, [parachain](#parachain) teams agree to lock up (or
  bond) a portion of DOT tokens for the duration of the lease. While bonded for a lease, the DOT cannot be used for
  other activities like staking or transfers.

- [Collator](#collator) slot auction (selection mechanism). It is used as a deposit to become a collator. Candidates can
  register by placing the minimum bond. Then, if an account wants to participate in the [collator](#collator) slot
  auction, they have to replace an existing candidate by placing a greater deposit (bond).

### Collateral

[Collaterals](https://spec.filecoin.io/#section-glossary.collateral) are assets that are locked up or deposited as a
form of security to mitigate risks and ensure the performance of certain actions. Collateral acts as a guarantee that an
individual will fulfill their obligations. Failing to meet obligations or behaving maliciously can result in the loss of
staked assets or collateral as a penalty for non-compliance or misconduct by [slashing](#slashing).

### Collator

[Collators](https://wiki.polkadot.network/docs/learn-collator) maintain [parachains](#parachain) by
collecting [parachain](#parachain) transactions from users and producing state transition proofs
for [Relay Chain](#relay-chain) validators. In other words, collators maintain [parachains](#parachain) by
aggregating [parachain](#parachain) transactions into [parachain](#parachain) block candidates and producing state
transition proofs (Proof-of-Validity, PoV) for validators. They need to provide a financial
commitment ([collateral](#collateral)) to ensure they are incentivized to perform their duties correctly and
to dissuade malicious behavior.

### Committed Capacity

The [Committed Capacity](https://spec.filecoin.io/#section-glossary.capacity-commitment) (CC) is one of three types of
deals in which there is effectively no deal, and the [Storage Provider](#storage-provider) stores random data inside the
sector instead of customer data.

If a [storage provider](#storage-provider) doesn't find any available deal proposals appealing, they can alternatively
make a capacity commitment, filling a sector with arbitrary data, rather than with client data. Maintaining this sector
allows the [storage provider](#storage-provider) to provably demonstrate that they are reserving space on behalf of the
network.

### Crowdloan

Projects can raise DOT tokens from the community
through [crowdloans](https://wiki.polkadot.network/docs/learn-crowdloans). Participants pledge their DOT tokens to help
the project win a parachain slot auction. If successful, the tokens are locked up for the duration of the parachain
lease, and participants might receive rewards or tokens from the project in return.

### Full Node

A device (computer) that fully downloads and stores the entire blockchain of the parachain, validating and relaying
transactions and blocks within the network. It is one of the [node](#node) types.

### Invulnerable

A status assigned to certain [collators](#collator) that makes them exempt from being removed from the active set of
[collators](#collator).

### Node

A device (computer) that participates in running the protocol software of a decentralized network; in other words, a
participant of the blockchain network who runs it locally.

### Parachain

A parachain is a specialized blockchain that runs in parallel to other parachains within a larger network, benefiting
from shared security and interoperability, and can be validated by the validators of the [Relay Chain](#relay-chain).

### Polkadot

“Layer-0” blockchain platform designed to facilitate interoperability, scalability and security among different
“Layer-1” blockchains, called [parachains](#parachain).

## Proofs

Cryptographic evidence used to verify that storage providers have received, are storing, and are continuously
maintaining data as promised.

There are two main types of proofs:

- <a name="porep"></a> **Proof-of-Replication (PoRep):** In order to register a sector with the network, the
  sector has to be sealed. Sealing is a computation-heavy process that produces a unique representation of the data in
  the form of a proof, called Proof-of-Replication or PoRep.

- <a name="post"></a> **Proof-of-Spacetime (PoSt):** Used to verify that the storage provider continues to store the
  data over time. [Storage providers](#storage-provider) must periodically generate and submit proofs to show that they
  are still maintaining the stored data as promised.

### Relay Chain

The Relay Chain in [Polkadot](#polkadot) is the central chain (blockchain) responsible for the network's shared
security, consensus, and cross-chain interoperability.

### Session

A predefined period during which a set of [collators](#collator) remains constant.

### Slashing

The process of penalizing network participants, including [validators](#validators), [nominators](#nominators),
and [collators](#collator), for various protocol violations. These violations could include producing invalid blocks,
equivocation (double signing), inability of the [Storage Provider](#storage-provider) to [prove](#proofs) that the data
is stored and maintained as promised, or other malicious activities. As a result of slashing, participants may face a
reduction in their [staked](#staking) funds or other penalties depending on the severity of the violation.

### Slot Auction

To secure a [parachain](#parachain) slot, a project must win an auction by [pledging](#term-to-avoid-pledge) (locking
up) a significant amount of DOT tokens. These tokens are used as [collateral](#collateral) to secure the slot for a
specified period. Once the slot is secured, the project can launch and operate its [parachain](#parachain).

### Staking

Staking is the process where DOT holders lock up their tokens to support the network's security and operations. In
return, they can earn rewards. There are two main roles involved in staking:

- <a name="validators"></a>**Validators**
  : Validators are responsible for producing new blocks, validating transactions, and securing the
  network. They are selected based on their stake and performance. Validators need to run a [node](#node) and have the
  technical capability to maintain it.

- <a name="nominators"></a>**Nominators**: Nominators support the network by backing (nominating) validators they trust
  with their DOT tokens. Nominators share in the rewards earned by the validators they support. This allows DOT holders
  who don't want to run a validator node to still participate in the network's security and earn rewards.

Our parachain will use staking to back up the [collators](#collator) in a similar way as "Nominators" do.
In this regard, the role of "Nominators" will fall to [Storage Providers](#storage-provider), while the role of "
Validators" will be assigned to [Collators](#collator) accordingly.

### Storage Provider

The user who offers storage space on their devices to store data for others.

### Storage User

**_Aka Client:_** The user who initiates storage deals by providing data to be stored on the network by the [Storage
Provider](#storage-provider).

### System Parachain

[System-level chains](https://wiki.polkadot.network/docs/learn-system-chains) move functionality from
the [Relay Chain](#relay-chain) into [parachains](#parachain), minimizing the administrative use of
the [Relay Chain](#relay-chain). For example, a governance [parachain](#parachain) could move all
the [Polkadot](#polkadot) governance processes from the [Relay Chain](#relay-chain) into a [parachain](#parachain).

## Anti-Glossary

This section lists terms that should not be used within the project, along with preferred alternatives.

### Term to Avoid: Miner

In [Filecoin](https://spec.filecoin.io/#section-glossary.filecoin), a "Lotus Miner" is responsible for storage-related
operations, such as sealing
sectors ([PoRep (Proof-of-Replication)](https://spec.filecoin.io/#section-algorithms.pos.porep)), proving storage
([PoSt (Proof-of-Spacetime)](https://spec.filecoin.io/#section-algorithms.pos.post)), and participating in
the [Filecoin](https://spec.filecoin.io/#section-glossary.filecoin) network as a storage miner.

**Reason**: In the [Filecoin](https://spec.filecoin.io/#section-glossary.filecoin) network, the miner plays the roles of
both [storage provider](#storage-provider) and block producer simultaneously. However, in the [Polkadot](#polkadot)
ecosystem, this term cannot be used because there are no block producers in [parachains](#parachain);
the [Relay Chain](#relay-chain) is responsible for block production. [Parachains](#parachain) can only prepare block
candidates via the [Collator](#collator) node and pass them to the [Relay Chain](#relay-chain).

### Term to Avoid: Pledge

It's better to apply this term within its proper context rather than avoiding it altogether. It's easy to confuse it
with [staking](#staking), but they have distinct meanings.

**Reason**: Pledging generally refers to locking up tokens as [collateral](#collateral) to participate in certain
network activities or services like: [Parachain Slot Auctions](#slot-auction) and [Crowdloans](#crowdloan).