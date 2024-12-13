# RFC-003: Randomness

Author: José Duarte — @jmg-duarte
Date: 6/12/24

## Abstract

This document covers existing randomness methods in Polkadot and discusses which
would be appropriate to the Polka Storage project. I propose the usage of BABE's
AuthorVRF which aims to be somewhere between BABE's Epoch randomness and
the Local-VRF approach, providing a somewhat trusted and unpredictable
source of fresh randomness.

There is an implict assumption that the Storage Provider only observes the
finalized chain.

## Introduction

The Polka Storage project requires randomness as a form of creating challenges
for Storage Providers, with the aim of stopping them from discarding the files
they are supposed to store, by pre-computing proofs for upcoming challenges.
Achieving this randomness is not trivial — for example, we cannot allow the
Storage Provider to generate the randomness alone since they may be dishonest
and generate values that allow them to fake the storage proofs; furthermore by
their very nature, blockchains must be somewhat deterministic and as such,
randomness methods achieved on-chain are not suitable for all use cases. For
example consider an application that requires multiple rounds of randomness,
to achieve that you would generally reach out to a pseudo random number
generator (PRNG), however that PRNG requires a seed to get started, and
on-chain, everyone can see that seed and as such, predict the outcomes; now
consider that you changed your code to change the seed every time a new block
is generated, while the seed is no longer predictable, it is still possible
for users to read it (it is the latest block hash after all) and predict
future outcomes while a new block is not available — as such, most methods
discussed in this RFC are not suitable for cases where multiple random values
are required (such as casinos where multiple rolls require multiple rounds
over a single seed), however they are suited for cases where a single random
value is required.

## Terminology

* SP — Storage Provider
* VRF — Verifiable Randomness Function
* PoRep — Proof of Replication
* PoSt — Proof of Spacetime
* drand — Distributed Randomness Beacon
* PRNG — Pseudo Random Number Generator

## The Problem

The Polka Storage Parachain SPs generate proofs that they are, in fact, storing
the files they committed to store. These proofs are PoRep and PoSt and they sit
at the core of the system — breaking them means breaking the system, allowing
storage to be proven without any actual storage being done.

Randomness plays a different role in each proof; for PoRep, the randomness plays
a crucial role in the generation of a unique sector, avoiding the possibility
that the storage is outsourced or duplicated to another provider — that is, two
providers may store the same file, while outsorcing the actual storage to a
co-located server, effectively scamming the final customer which thinks their
data is replicated among two distinct providers but in reality, it was only
stored once.

As for PoSt, randomness is used for challenges, these stop the SP from
pre-computing proofs and thus, faking their stored files. As long as the SP is
unable from accurately guessing the next set of challenges, the PoSt should
suffice as proof that the files are still stored over time.

In Filecoin, these randomness values come from two sources, VRFs and drand, the
former are used for PoRep while the latter is used for PoSt. The VRFs are used
as part of the consensus mechanism to avoid fork attacks. In our case, we
do not need to worry about that as Polkadot provides mechanisms to handle
consensus, both at the parachain and relay chain levels.

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
the only ones that may alter the block contents (e.g. included extrinsics) to
change the output hash. This change is not detectable by the validators
because it keeps validity of the block, as such the final block (barring
other changes) will be a "normal" block that eventually gets integrated in
the chain.

In the Polka Storage Parachain, the SPs are not Collators, but nothing stops
a SP from running a malicious Collator node, meaning they can influence the
value of a block for their own gain; however, collator selection has two
features that play against this — invulnerables, which are collators we trust
by default and the fact that multiple collators propose a block, making the
determining the winning block a non-deterministic operation. As such, as long
as there is at least one honest collator that is able to submit a winning block
every so often, a dishonest SP will eventually be detected and punished.

As an exercise, consider that an attacker is able to bias the hash of the
finalized block; meaning they are able to select an hash that obeys certain
rules but are not able to fully manipulate the hash, meaning they cannot
generate any hash they desire. This ability should not affect the guarantees
of the PoRep unless the used hashing function has collisions, as the PoRep
generates an unique copy of the sealed data, meaning that biasing the block
hash with the purpose of manipulating the PoRep is an exercise in futility.
The PoSt may be susceptible to biasing as the attacker may only ever test a
subset of all the stored data, effectively allowing for a part of the stored
data to be thrown out; however, to fully carry out this attack the attacker
must be able to bias all produced blocks which then must be finalized, a
process over which the attacker has no control unless they control or are
colluding with 2/3s of the validators [DOT1][7] (at which point, you have
bigger concerns than a single SP not proving their files).

### BABE Randomness

As part of BABE consensus, VRFs are used to select the validator that will
propose a block — the VRF [randomness is exposed in the relay chain proof][1],
meaning we just need to "pull" that randomness into our parachain; this is
the approach taken by Moonbeam ([MB1][2], [MB2][3]), and StorageHub ([SH1][4],
[SH2][5]).

This approach involves injecting the BABE randomness value into parachain
storage, then, when necessary fetch it for usage. The main advantage is that
this approach is verifiable, the main disadvantage is that the randomness
takes a long time to refresh as it is dependent on BABE epochs (which last
4 hours on mainnet), meaning that we are unable to generate unpredictable
values for 4 hours.

BABE offers a AuthorVRFRandomness which, in short, consists of having the
block author run a VRF over some inputs from the block being built [BABE1][8].
This value is stored in BABE's storage and is somewhere between having the
collator create a VRF (see [Local-VRF](#local-vrf)) and BABE's Epoch
randomness.

### Local-VRF

The Local-VRF approach is based on the ideas behind BABE's AuthorVRFRandomness,
however, it seems to use a default hash value as input for the randomness
[VRF1][9]. This approach is simpler to implement and understand, having the
block author build on the previous VRF also reduces the attack surface for
manipulation effectively creating a chain much like Filecoin's.

However, depending on implementation, this approach may be biasable. If the
inputs are manipulatable by the block author, they may generate ones that benefit
them, however, similar to the block hash approach, a large part of the network
must collude to effectively break the guarantees of this approach.

This approach however, is not feasible without complicated mechanisms that
may reduce the final node's security, this is due to the fact that the
runtime does not expose the mechanisms necessary to perform a VRF on it;
alternatives may exist in the form of off-chain workers, but even those
raise other security issues, for example, around trust.

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

## Conclusion

|            | Freshness    | Biasable | Effort   |
|------------|--------------|----------|----------|
| Block Hash | Every block  | Yes      | Low      |
| Local VRF  | Every block  | Yes*     | Medium   |
| Author VRF | Every block  | Yes*     | Medium** |
| BABE       | Every epoch  | No       | Medium   |
| drand      | Every block* | No       | Unclear  |

The author's recommendation is to make an effort moving forward with the
Local-VRF approach as it seems to be the most promising with regards to the
security/implementation effort trade-off.

\* This approach may be biasable depending on how the VRF is constructed,
   if all VRFs use the previous VRF as their input seed this reduces the
   possibility of bias and effectively creates a "chained-VRF" — this is
   the approach taken by Filecoin for PoRep's randomness.

\** This approach was implemented in

## References

[1]: https://github.com/paritytech/cumulus/issues/463
[2]: https://github.com/moonbeam-foundation/moonbeam/blob/20119cddc4e7074878e545c8292c1fac07f1e4d3/runtime/moonbeam/src/lib.rs#L1309-L1357
[3]: https://github.com/Moonsong-Labs/moonkit/blob/main/pallets/randomness/src/lib.rs
[4]: https://github.com/Moonsong-Labs/storage-hub/blob/e3d42d3b262bead49dafc42bdff22fd5c6105660/runtime/src/configs/mod.rs#L391-L461
[5]: https://github.com/Moonsong-Labs/storage-hub/blob/e3d42d3b262bead49dafc42bdff22fd5c6105660/pallets/randomness/src/lib.rs#L8
[6]: https://github.com/ideal-lab5/idn-sdk/tree/main/pallets/drand
[7]: https://spec.polkadot.network/sect-finality#sect-block-finalization
[8]: https://docs.rs/pallet-babe/latest/src/pallet_babe/lib.rs.html#341-392
[9]: https://github.com/Moonsong-Labs/moonkit/blob/main/pallets/randomness/src/lib.rs#L304
