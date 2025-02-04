# RFC-004: P2P Information Exchange

Author: Aidan Sullivan - @aidan46
Date: 31/01/25

## Abstract

Currently, bootstrap nodes are only aware of registrations, [Peer ID][1] and [multiaddress][2]'s, that they hold themselves because they do not share this information with each other.
This document discusses improvements on the P2P bootstrapping protocol to share information about peers with each other.
The documents outlines 2 different solutions; having bootstrap nodes exchange information between each other and connecting to all bootstrap nodes when resolving a [Peer ID][1] to a [multiaddress][2].
The preferred solution would be to have bootstrap nodes gossip amongst themselves to ensure that the registration information is replicated and recoverable.

## Problem Statement

The current design for the P2P information exchange is to have some storage providers bootstrap nodes and other register with these bootstrap nodes using the rendezvous protocol.
This is a problem for clients looking for storage providers, they must first resolve the storage provider [Peer ID][1] into a [multiaddress][2], however, as it currently stands, collators are only aware of the storage providers that registered directly with them.

## Proposals

### Bootstrap Node Collator Service

[Collators][3] maintain parachains by collecting parachain transactions from users and producing state transition proofs for relay chain validators.
Since collators are already well known within the network it makes sense for them to be bootstrap nodes an contain the P2P information.
The collators will act as [rendezvous][4] servers at which all the storage provider will register, providing information about their [Peer ID][1] and [multiaddress][2].
The bootstrap node will be implemented as a service into the collator node.

Solving the bootstrap nodes not knowing the registration information that they each hold there are 2 options.

#### 1. Sharing information amongst bootstrap nodes

When the bootstrap nodes start up they will connect to each other over the P2P network.
After connecting with each other they will utilize libp2p's [gossipsub][5] protocol to relay registration information between themselves.
Information about new and updated registrations will be published over a topic and the bootstrap nodes will maintain an in-memory database that holds this information.

The advantage of this solution is that all the registration information is replicated.
This duplication makes sure that if a bootstrap node were to go down it can reconstruct the registration information from the other bootstrap nodes.
Furthermore, duplication ensures that when a client requests a [Peer ID][1] to [multiaddress][2] mapping it only has to request this information from a single bootstrap node.

#### 2. Clients connecting to all bootstrap nodes

Bootstrap nodes split the registration information, holding only information of nodes that register with them.
Requiring the client, who wants to get a [Peer ID][1] to [multiaddress][2] mapping, to request this information from all bootstrap nodes.
This would require the bootstrap nodes to each have a database that holds their information in the event that they would go down.

The advantages of this that the implementation is simple, as it does not require any gossip between the bootstrap nodes.

The disadvantage of this solution is that resolving a [Peer ID][1] to a [multiaddress][2] will be slower as it requires connecting to all nodes in the worst-case scenario.
Another disadvantage is that this requires bootstrap nodes to have an on-disk database, so that information does not get lost in the event that they go down, which requires I/O to move data into memory which can be slow.
Having a on-disk database also requires additional setup for the collator which is not preferred.
Furthermore, we cannot dictate which bootstrap node storage provider will register with.
This could result in a single bootstrap node holding the majority of the peer information.

## Conclusion

This document covered two different solutions for peer information availability, exchanging information between bootstrap nodes or solving the issue on the client side.

Taking both solutions into consideration, exchanging information between bootstrap nodes is the better solution.
This solution does not require a database and handles recovery by gossiping information between bootstrap nodes.
Furthermore, this solution simplifies the client side by only having to connect to a single bootstrap node to request any information needed.

[1]: https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md#peer-ids
[2]: https://docs.libp2p.io/concepts/fundamentals/addressing/
[3]: https://wiki.polkadot.network/docs/learn-collator
[4]: https://github.com/libp2p/specs/blob/master/rendezvous/README.md
[5]: https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/README.md
