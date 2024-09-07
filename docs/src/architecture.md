# Architecture Overview

The Polka Storage parachain is, just like other parachains, composed of collators that receive extrinsics calls,
and through them perform state transitions.

## System Overview

<img src="images/architecture/system_overview.svg" >

From left to right, we have validators (represented by a single one as only one validates blocks at a time),
collators, storage providers and their respective storage.

The validators are out of the scope of this project, validating the blocks submitted by a collator selected at random
(the selection process is not covered here).

The collators run our parachain runtime and process extrinsic calls from the storage providers —
such as proof of storage submissions.
The storage providers are independent of the collators, being controlled by arbitrary people that provide storage to the system.
Storage management is left to the storage providers, being responsible to keep their physical system in good shape to serve clients.

## Collator Overview

<img src="images/architecture/collator_overview.svg" >

Taking a deeper dive into the collator architecture, our main focus is on developing the core parachain pallets —
currently, the storage provider and market pallets.
The collator automatically exposes a JSON-RPC API for the extrinsics calls,
this API can then be called from a library such as `storagext`, Polkadot.js,
or even just with raw HTTP and JSON-RPC payloads.

The storage provider interacts with the collator through the `storagext` API,
first registering themselves in the network, registering deals and eventually submitting proofs for validation.

The validation of the proofs is not done by the collator per-se, but rather by an offchain worker,
that can be hosted along with the collator or not. This is due to the WASM runtime limitations —
we cannot run proof verification inside it.

## Storage Provider Overview

<img src="images/architecture/storage_provider_overview.svg">

The storage provider is composed of the proof subsystem which proves the storage,
a cron-like service that schedules the proving process and the CAR library that validates CAR files submitted by the user.

The client submits prepared CAR files over Graphsync, which the CAR library then validates — verifies the contents match the CID.

After the data has been submitted, it needs to be proven, the cron will schedule the proving process for each deal accepted from the clients.

## Resources on Parachains

Reading:
* [Parachains' Protocol Overview](https://wiki.polkadot.network/docs/learn-parachains-protocol)
* [The Path of a Parachain Block](https://polkadot.com/blog/the-path-of-a-parachain-block)

Videos:
* [Introduction to Polkadot, Parachains, and Substrate](https://www.youtube.com/live/gT-9r1bcVHY?si=dmCJyWB5w2NY1bnu&t=1670)
* [The Path of a Parachain Block - Joe Petrowski](https://www.youtube.com/watch?v=vRsBlVELQEo)
* [The Path of a Parachain Block on Polkadot and Kusama Network](https://www.youtube.com/watch?v=m0vxqWwFfDs)

