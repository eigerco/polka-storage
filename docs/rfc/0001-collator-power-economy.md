# Collator Power Economy

## Proposal

Collators are separate from Storage Providers. They are mostly sponsored by Polkadot's treasury and governed by the Fellowship.
There is a fixed list of Invulnerables selected by the governance and on top of that, a fraction of Collators that anyone can become by staking DOTs and running a Collator node. This is no different from other System Parachains (i.e. AssetHub, BridgeHub).
As an extension to that, Storage Providers can as well stake their DOTs on Collators, to help them win a slot in a session. 
Other network participants should not have an option to influence the Collator selection.

The Collator Pool should approximately have:
- 20 collators for the parachain,
- of which 15 are Invulnerable,
- 5 are elected by bond with possibly delegated tokens by Storage Providers.

## Context

Current System Parachain Collators are sponsored by the [treasury][2] and Polkadot's governance, as there is [little to none][1] economic incentive to run a Collator Node - no inflationary reward system. They are always running at loss, because they're expensive, resource-intensive and fees earned from collating blocks are not substantial.
Our system parachain is no exception. In addition to Polkadot Treasury's sponsored Collators, anyone can also become a collator if they want to and bond funds to participate in Candidates election.

[The usual reasons][3] for setting non-treasury backed Collator node are valid, so we can imagine a scenario where a Storage Provider wants to have a say in including their transactions in the blocks. As running Storage Provider is expensive enough, they should have a possibility to stake their funds to support **their trusted** collator. The main incentive for that is not earning rewards from block candidate production, but just making sure there is a Collator on their side which is not censoring transactions.

## Alternatives

NOTE: each of those approaches assume that amount of storage space a Storage Provider provides DOES NOT directly affect staking or collator selection (in contrast to the Crust's [GPoS][4] and [FileCoin's consensus][5]).
It can affect it indirectly, i.e. when a Storage Provider has lots of deals, hence lots of tokens, so they can stake it on a Collator.

### 1. There are invulnerables, but only Collators can stake tokens, they cannot be nominated.

If Storage Providers weren't able to stake tokens and didn't trust the network's invulnerables, they'd have no other option but to run their own Collator Node. There is a limited number of slots for Collators, so Storage Providers would compete between each other and need to stake lots of tokens for little to no guarantee. 

### 2. There are no Invulnerables, collators are run by the community.

It would be a charity work, no one wants to become the Collator in this model. If Storage Providers want to earn money, they'd need to spin up Collator Nodes to make sure the network is functioning and have all of the drawbacks of alternative 3.

### 3. There are no Invulnerables, each Storage Provider runs as Collator.

There is no economic incentive for Storage Provider's Collator Node to be honest and include everyone's transactions.
Running a Collator Node is expensive and the rewards for that are minimal, so each Storage Provider would only include their own transactions. The biggest Storage Providers would centralize the power. 

## FAQ

### 1. Who is a Collator? 

Collators are network participants which produce block candidates to be backed by validators.
Collators run both **a relay chain full node** and **a parachain full node**.
They do not concern themselves with finality - a decision whether the block will be definitely included in the blockchain.
Their only role is to **produce parachain block candidates**. The security is delegated to **relay chain validators**.
**Producing parachain block candidates** means gathering the transactions from the gossip across the parachain nodes and **collating** them into a block. 

A decision about which collator produces a block and communicates with the relay chain boils down to **block authoring** algorithm. 
Example block authoring algorithms are: BABE, AuRa. The algorithms solve the distributed consensus problem and selecting a node which will be producer of a given block. Each block may have a different block author.
However the block authoring algorithms require having a set of validators (collators) before they start the election.
There needs to be a pallet, that feeds that data into the algorithm for it to be able to select the block producers.

Those algorithms **do NOT** detect misbehaviours in the network, e.g: trying to include a malicious block with double spending. They cannot know that, as they're only **selecting** a block producer. The selected block producer (Collator) is forwarding the block for validation to the relay chain's validator. 

**Validator** validates a block by running a parachain's **state transition function** (runtime), on their own and confirming whether the business logic contained in the block (they execute block's transactions) is sound, valid and according to the **Proof of Validity**. If the block is correct, then it must be backed up by a majority of validators and approved.
If the block is incorrect, then it won't pass validator's backing and approvals and won't be included as an available parablock.
It's simply discarded, so the next transaction won't use it as a parent.

If a relay chain's validators back up a malicious block, they are slashed by majority of validators, losing part of their stake and eventually being kicked out.

We'd like to prevent DoS attacks on the network where parachain is not able to progress/censors transactions, because some Collators are constantly elected because of their high stakes. 
In case of other System Parachains it's solved by having a list of governance elected Collators - [Invulnerables][1], which are trusted to be behaving correctly.
A different mechanism to prevent those used by some [parachains with inflationary reward system][6] is indirect, but effective one. They keep track when was the last time a Collator authored a block, if it did not happen in last x hours, we kick him out and slash 1% of this DOTs.

### 2. Why running too many collators is a bad idea?

1. To produce a block candidate, each Collator needs to gather transactions. If there are a lot of transactions happening and there is a congestion, many of the collators may not receive some of the transactions. They'd not have agreement on how the chain should look like, so it'd slow down the network before it finally settles on the valid chain after receiving blocks from the relay chain. 
It can be mitigated with collator selection logic, at each point in time there may be a set of Candidates from which we select a Collator set for each session and then the number of active collators would be reduced.

2. We assume there will be lots of Storage Providers in the parachain, so there'd be lots of Collators.
If each Storage Provider was running a Collator then they'd need to have very powerful machines which might be the blocker, as Collators on their own are resource intensive. It'd complicate things for our clients, maybe to handle those they'd need to run a fleet of nodes, not just a node. We cannot know at this stage [`7`][7], [`8`][8].

### 3. What's the difference between BABE and AuRa

Both [BABE][9] and [AuRa][10] are block authoring algorithms. 

AuRa is an algorithm that has a set of validators and selects them in round-robin fashion. A set of validators must be known before each session starts, then the time is divided into slots, where for each slot a block producer is selected.

BABE also has a set of validators, but additionaly each validator is assigned a weight and this weight is combined with a [VRF][11] to decide whether block is produced.

Overall both work, BABE is advertised as more scalable and secure. However we got security and scalability provided by the relay-chain.
Honestly, it's not a blocker, as those can be switched out whenever and they are considered separate to rest of the mechanisms.
We can go with AuRa (as AssetHub does, another system parachain) and later replace it.

### 4. What does option `--collator` on a parachain node do?

`--collator` [implies][12] `--validator`.
When we run a node in `--collator` mode, it's role is set to `authority`.
It runs both full node for the relay chain and full node for the parachain.
When the node is run as authority, collator it runs a [Collator Service][13] which works as a proxy between parachain and the relay chain.

### 5. Why only Storage Providers should be able to stake tokens on Collators?

Storage Providers may have a need reason to stake on Collators to make sure their Storage Deals are being included.
There is no economic incentive to to that, as the rewards from blocks are not substantial.
The only reason other network participants would want to nominate Collators is to introduce some kind of malicious Collator, 
and not running their own node. It doesn't make much sense and staking only by Storage Providers is an additional safeguard.

[1]: https://github.com/polkadot-fellows/RFCs/blob/main/text/0007-system-collator-selection.md
[2]: https://polkadot.polkassembly.io/referenda/288
[3]: https://github.com/polkadot-fellows/RFCs/blob/main/text/0007-system-collator-selection.md#explanation
[4]: https://polkadot-blockchain-academy.github.io/pba-book/economics/economics-of-polkadot/page.html
[5]: https://spec.filecoin.io/#section-algorithms.expected_consensus
[6]: https://docs.astar.network/docs/build/nodes/collator/learn/
[7]: https://forum.polkadot.network/t/determine-collator-node-minimal-performance-requirement/613/6
[8]: https://docs.astar.network/docs/build/nodes/collator/requirements/
[9]: https://research.web3.foundation/Polkadot/protocols/block-production/Babe
[10]: https://openethereum.github.io/Aura.html
[11]: https://en.wikipedia.org/wiki/Verifiable_random_function
[12]: https://github.com/paritytech/polkadot-sdk/blob/5fb4c40a3ea24ae3ab2bdfefb3f3a40badc2a583/cumulus/client/cli/src/lib.rs#L356
[13]: https://github.com/paritytech/polkadot-sdk/blob/5fb4c40a3ea24ae3ab2bdfefb3f3a40badc2a583/cumulus/client/collator/src/service.rs