# RFC: Randomness

## Abstract

This document covers existing randomness methods in Polkadot and discusses which
would be appropriate to the Polka Storage project.

## Introduction

The Polka Storage project requires randomness as a form of creating challenges
for Storage Providers, with the aim of stopping them from discarding the files
they are supposed to store, by pre-computing proofs for upcoming challenges.
Achieving this randomness is not trivial — for example, we cannot allow the
Storage Provider to generate the randomness alone since they may be dishonest
and generate values that allow them to fake the storage proofs; furthermore by
their very nature, blockchains must be deterministic and as such, randomness
methods achieved on-chain are not suitable for all use cases.

## Terminology

* SP — Storage Provider
* VRF — Verifiable Randomness Function
* PoRep — Proof of Replication
* PoSt — Proof of Spacetime
* drand — Distributed Randomness Beacon

## The Problem

The Polka Storage Parachain SPs generate proofs that they are, in fact, storing
the files they committed to store. These proofs are PoRep and PoSt and they sit
at the core of the system — breaking them means breaking the system, allowing
storage to be proven without any actual storage being done.

Randomness plays a different role in each proof; for PoRep, the randomness plays
a crucial role in the generation of a unique sector, avoiding the possibility
that the storage is outsourced or duplicated to another provider — meaning
two providers share the same storage space, keeping a single copy of the data;
for PoSt, SPs receive random challenges, these ensure that the SP cannot
pre-compute proof values, allowing them to discard the data while faking storage.

In Filecoin, these randomness values come from two sources, VRFs and the drand,
the former are used for PoRep while the latter is used for PoSt. The VRFs are
used also used as part of the consensus mechanism to avoid fork attacks. In our
case, we should not need to worry about that as Polkadot provides mechanisms to
handle consensus, both at the parachain and relay chain levels.

The problem lies in ensuring that we have randomness sources that are "random
enough" to keep the proofs secure — i.e. impossible to fake or pre-compute.

## Existing Randomness Methods

As in all blockchains, finding randomness in Polkadot is a complicated task;
depending on the final use case, the randomness level may not be suited for
the final application. This section will cover the randomness sources we have
at our disposal.

### Block Hash

While deterministic — i.e. for two equal inputs (blocks), the output of the
hashing function will always be the same — block hashes are not predictable,
in other words, without running the hashing function, you cannot predict the
final hash. While block hashes are not predictable, they are biasable by the
block author, being possible to change small aspects of the block to influence
the final hash, until a "more desirable" one is found.

In Polkadot, Collators are responsible for block authoring, as such, they are
the only ones that may alter the block to change the output hash. This change
is not detectable by the validators because it keeps the block integrity,
as such the final block (barring other changes) will be a valid block that
eventually gets integrated in the chain.

In the Polka Storage Parachain, the SPs are not Collators, but nothing stops
a SP from running a malicious Collator node, meaning they can influence the
value of a block for their own gain; however, collator selection has two
features that play against this — invulnerables, which are collators we trust
by default and the fact that multiple collators propose a block, making the
determining the winning block a non-deterministic operation. As such, as long
as there is at least one honest collator that is able to submit a winning block
every so often, a dishonest SP will always be detected.

Furthermore, as long as we use the latest block hash available (i.e. the
parent block of the one being produced) even if the SP can bias the resulting
hash, it will only provide them with an advantage of 6 seconds (block production
time) + challenge lookback (10 minutes for the production testnet); additionaly
this advantage does not stack, meaning that being 10 (or any arbitrary amount)
minutes early to a proof does not enable the SP to start the next proof 10 (or
any arbitrary amount) minutes early.

### BABE Randomness

As part of BABE consensus, VRFs are used to select the validator that will
propose a block — the VRF [randomness is exposed in the relay chain proof][1],
meaning we just need to "pull" that randomness into our parachain (easier
said than done); this is the approach taken by Moonbeam ([MB1][2], [MB2][3]),
and StorageHub ([SH1][4], [SH2][5]).

This approach involves injecting the BABE randomness value into parachain
storage, then, when necessary fetch it for usage. The main advantage is that
this approach is verifiable, the main disadvantage is that the randomness
takes a long time to refresh as it is dependent on BABE epochs (which last
4 hours on mainnet).

### Local-VRF

The Local-VRF is another Moonbeam solution to the randomness problem, this
approach is simpler, in that the collator producing the block simply generates
and stores a VRF output on-chain.

This approach suffers the same issue that the block hash suffers — it is
biasable, e.g. the collator may generate multiple values until one that benefits
them comes up. It may be possible to mitigate this by using the previous VRF
value or block hash as input to the VRF — this limits malicious collators since
they are no longer able to "roll the dice" multiple times until a suitable value
emerges.

This approach also carries the issue that "non-use-case-specific" VRF
implementations are not common, while VRFs are in production use across
blockchains, however none of them are "plug and play".

### drand Pallet

The drand network is the randomness source of multiple blockchain networks,
however, Polkadot is not one of them. Building a bridge to drand is not a
trivial task, but some folks have started building just that. The
[drand pallet][6] provides randomness from drand's Quicknet while being
pallet-randomness compliant.

Validation of the pulses can be done as part of the proof of validity of the
parachain block, as such, if some collator submits a "fake" pulse, the validator
should be able to detect and punish the collator accordingly.

The main issue with the crate is that it is not ready for production use.

## References

* <https://polkadot.com/blog/the-path-of-a-parachain-block>

[1]: https://github.com/paritytech/cumulus/issues/463
[2]: https://github.com/moonbeam-foundation/moonbeam/blob/20119cddc4e7074878e545c8292c1fac07f1e4d3/runtime/moonbeam/src/lib.rs#L1309-L1357
[3]: https://github.com/Moonsong-Labs/moonkit/blob/main/pallets/randomness/src/lib.rs
[4]: https://github.com/Moonsong-Labs/storage-hub/blob/e3d42d3b262bead49dafc42bdff22fd5c6105660/runtime/src/configs/mod.rs#L391-L461
[5]: https://github.com/Moonsong-Labs/storage-hub/blob/e3d42d3b262bead49dafc42bdff22fd5c6105660/pallets/randomness/src/lib.rs#L8
[6]: https://github.com/ideal-lab5/idn-sdk/tree/main/pallets/drand
