# Proof of Storage

> [!NOTE]
> Some terms used in this document are described in the [design document](./DESIGN.md#constants--terminology)

In our parachain within the Polkadot ecosystem, storage providers are required to prove that they hold a copy of the data they have committed to storing at any given point in time. This proof is achieved through a mechanism known as 'challenges.' The process involves the system posing specific questions to the storage providers, who must then provide correct answers to prove they are maintaining the data as promised.

To ensure the integrity and reliability of these proofs, the challenges must:

1. Target a Random Part of the Data: The challenge must be directed at a randomly selected portion of the stored data.
2. Be Timed Appropriately: Challenges must occur at intervals that make it infeasible, unprofitable, or irrational for the storage provider to discard the data and retrieve it only when challenged.

General Proof-of-Storage (PoS) schemes are designed to allow users to verify that a storage provider is indeed storing the outsourced data at the time a challenge is issued. However, proving that data has been stored continuously over a period of time poses additional challenges. One method to address this is to require repeated challenges to the storage provider. However, this approach can lead to high communication complexity, which becomes a bottleneck, especially when storage providers must frequently submit proofs to the network.

To overcome the limitations of continuous Proof-of-Storage, there is proof called Proof-of-Spacetime (PoSt). PoSt allows a verifier to check whether a storage provider has consistently stored the committed data over Space (the storage capacity) and Time (the duration). This method provides a more efficient and reliable means of proving data storage over extended periods, reducing the need for constant interaction and lowering the overall communication overhead.

By implementing PoSt, our parachain ensures that storage providers maintain the integrity of the data they store, providing a robust and scalable solution for decentralized storage within the Polkadot ecosystem.

## Proof of Replication

To register a storage sector with our parachain, the sector must undergo a sealing process. Sealing is a computationally intensive procedure that generates a unique proof called Proof-of-Replication (PoRep), which attests to the unique representation of the stored data.

The PoRep proof links together:

1. The data itself.
2. The storage provider who performs the sealing.
3. The time when the specific data was sealed by the specific storage provider.

If the same storage provider attempts to seal the same data at a later time, a different PoRep proof will be produced. The time is recorded as the blockchain height at which sealing took place, with the corresponding chain reference termed [SealRandomness](https://spec.filecoin.io/systems/filecoin_mining/sector/sealing/#section-systems.filecoin_mining.sector.sealing.randomness).

## Generating and Submitting PoRep Proofs

Once the proof is generated, the storage provider compresses it using a SNARK (Succinct Non-interactive Argument of Knowledge) and submits the result to the blockchain. This submission certifies that the storage provider has indeed replicated a copy of the data they committed to store.
Phases of the PoRep Process

The PoRep process is divided into two main phases:

1. Sealing preCommit Phase 1: In this phase, the PoRep encoding and replication take place, ensuring that the data is uniquely tied to the storage provider and timestamp.
2. Sealing preCommit Phase 2: This phase involves the generation of Merkle proofs and trees using the Poseidon hashing algorithm, providing a secure and verifiable method of proof generation.

By implementing PoRep within our parachain, we ensure that storage providers are accountable for the data they store, enhancing the integrity and reliability of our decentralized storage solution in the Polkadot ecosystem.

## Proof of Spacetime

From the point of committing to store data, storage providers must continuously prove that they maintain the data they pledged to store. Proof-of-Spacetime (PoSt) is a procedure during which storage providers are given cryptographic challenges that can only be correctly answered if they are actually storing a copy of the sealed data.

There are two types of challenges (and their corresponding mechanisms) within the PoSt process: WinningPoSt and WindowPoSt, each serving a different purpose.

- WinningPoSt: Proves that the storage provider has a replica of the data at the specific time they are challenged. A WinningPoSt challenge is issued to a storage provider only if they are selected through the [Secret Leader Election algorithm](https://eprint.iacr.org/2020/025.pdf) to validate the next block. The answer to the WinningPoSt challenge must be submitted within a short [deadline](./DESIGN.md#constants--terminology), making it impractical for the provider to reseal and find the answer on demand. This ensures that the provider maintains a copy of the data at the time of the challenge.
- WindowPoSt: Proves that a copy of the data has been continuously maintained over time. Providers must submit proofs regularly, making it irrational for them to reseal the data every time a WindowPoSt challenge is issued.

### WinningPoSt

> [!NOTE]
> This is not relevant for our implementation as block rewards are earned by Collators.

At the beginning of each block, a small number of storage providers are elected to validate new blocks through the Expected Consensus algorithm. Each elected provider must submit proof that they maintain a sealed copy of the data included in their proposed block before the end of the current block. This proof submission is known as WinningPoSt. Successfully submitting a WinningPoSt proof grants the provider a block reward and the opportunity to charge fees for including transactions in the block. Failing to meet the [deadline](./DESIGN.md#constants--terminology) results in the provider missing the opportunity to validate a block and earn rewards.

### WindowPoSt

WindowPoSt audits the commitments made by storage providers. Every 24-hour period, known as a [proving period](./DESIGN.md#constants--terminology), is divided into 30-minute, non-overlapping [deadline](./DESIGN.md#constants--terminology)s, totalling 48 [deadline](./DESIGN.md#constants--terminology)s per period. Providers must demonstrate the availability of all claimed [sectors](./DESIGN.md#constants--terminology) within this time frame. Each proof is limited to 2349 [sectors](./DESIGN.md#constants--terminology) (a partition), with 10 challenges per partition.
[Sectors](./DESIGN.md#constants--terminology) are assigned to [deadline](./DESIGN.md#constants--terminology)s and grouped into partitions. At each [deadline](./DESIGN.md#constants--terminology), providers must prove an entire partition rather than individual [sectors](./DESIGN.md#constants--terminology). For each partition, the provider generates a SNARK-compressed proof and publishes it to the blockchain. This process ensures that each sector is audited at least once every 24 hours, creating a permanent, verifiable record of the provider's commitment.
The more [sectors](./DESIGN.md#constants--terminology) a provider has pledged to store, the more partitions they must prove per [deadline](./DESIGN.md#constants--terminology). This setup necessitates ready access to sealed copies of each challenged sector, making it impractical for the provider to reseal data each time a WindowPoSt proof is required.

### Design of Proof-of-Spacetime

Each storage provider is allocated a 24-hour [proving period](./DESIGN.md#constants--terminology) upon creation, divided into 48 non-overlapping half-hour [deadline](./DESIGN.md#constants--terminology)s. Each sector is assigned to a specific [deadline](./DESIGN.md#constants--terminology) when proven to the chain and remains assigned to that [deadline](./DESIGN.md#constants--terminology) throughout its lifetime. [Sectors](./DESIGN.md#constants--terminology) are proven in partitions, and the set of [sectors](./DESIGN.md#constants--terminology) due at each [deadline](./DESIGN.md#constants--terminology) is recorded in a collection of 48 bitfields.

- Open: BlockNumber from which a PoSt Proof for this [deadline](./DESIGN.md#constants--terminology) can be submitted.
- Close: BlockNumber after which a PoSt Proof for this [deadline](./DESIGN.md#constants--terminology) will be rejected.
- FaultCutoff: BlockNumber after which fault declarations for [sectors](./DESIGN.md#constants--terminology) in the upcoming [deadline](./DESIGN.md#constants--terminology) are rejected.
- Challenge: BlockNumber at which the randomness for the challenges is available.

### PoSt Summary

- Storage providers maintain their [sectors](./DESIGN.md#constants--terminology) by generating Proofs-of-Spacetime (PoSt) and submitting WindowPoSt proofs for their [sectors](./DESIGN.md#constants--terminology) on time.
- WindowPoSt ensures that [sectors](./DESIGN.md#constants--terminology) are persistently stored over time.
- Each provider proves all their [sectors](./DESIGN.md#constants--terminology) once per [proving period](./DESIGN.md#constants--terminology), with each sector proven by a specific [deadline](./DESIGN.md#constants--terminology).
- The [proving period](./DESIGN.md#constants--terminology) is a 24-hour cycle divided into [deadline](./DESIGN.md#constants--terminology)s, each assigned to specific [sectors](./DESIGN.md#constants--terminology).
- To prove continuous storage of a sector, providers must submit a WindowPoSt for each [deadline](./DESIGN.md#constants--terminology).
- [Sectors](./DESIGN.md#constants--terminology) are grouped into partitions, with each partition proven in a single SNARK proof.

By implementing PoSt within our parachain, we ensure that storage providers are consistently accountable for the data they store, enhancing the integrity and reliability of our decentralized storage solution in the Polkadot ecosystem.
