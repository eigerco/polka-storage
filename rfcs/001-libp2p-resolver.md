# RFC-001: Peer ID to Multi-Address Resolver

## Abstract

This RFC proposes an application for resolving [Peer ID][5]s to [multi-addresses][4] in a [libp2p][1] network.
The application will utilize [libp2p][1]'s [rendezvous protocol][2] for peer discovery and the [identify protocol][3] for retrieving [multi-addresses][4] and related information.
By leveraging these protocols, the application aims to provide an efficient and seamless mechanism for peer-to-peer address resolution over the internet.

## Introduction

In [libp2p][1]-based networks, [Peer ID][5]s serve as unique identifiers for peers, while [multi-addresses][4] describe their network locations.
Clients need to resolve [Peer ID][5]s to [multi-addresses][4] so they can connect to and communicate with storage providers.
This application will provide a straightforward interface to resolve [Peer ID][5]s to [multi-addresses][4] by using [libp2p][1]'s discovery and information exchange mechanisms.

The application will operate over the internet, relying on bootstrap nodes as rendezvous points to join the network and initiate discovery.
It will integrate [libp2p][1]'s [rendezvous protocol][2] for locating peers and the [identify protocol][3] for exchanging relevant connection information.

## Bootstrap Nodes as Rendezvous Points

The application will rely on bootstrap nodes to initialize network interactions.
These nodes act as rendezvous points, enabling the clients to join the network and locate other peers.
Clients can configure the bootstrap nodes through a configuration file or command-line arguments.

### Bootstrap Node Incentives

Storage providers have a incentive to run bootstrap nodes in the P2P network because it increases their perceived trustworthiness and visibility within the ecosystem.
As bootstrap nodes, storage providers aid in peer discovery, enabling new and existing peers to connect more easily to the network.
This presence not only benefits the network's overall health and connectivity but also reflects positively on the storage provider's reliability and commitment to the decentralized storage ecosystem.
Trust is a critical factor for storage clients selecting which storage provider to store their data.
A provider that operates a bootstrap node demonstrates that they want to support the broader network, increasing confidence and trustworthiness among clients.
Bootstrap nodes are required for a seamless client experience by helping ensure that content can be discovered and retrieved efficiently.
This increase in trustworthiness can lead to increased client adoption and preference for storage providers that operate bootstrap nodes, giving these providers a competitive advantage in the ecosystem.

## Peer Discovery

The [identify protocol][3] plays a vital role in establishing communication of peer information.
This protocol allows peers to exchange metadata about themselves upon connection. This metadata includes:

- [Peer ID][5]: A unique identifier for the peer.
- [Multiaddrs][4]: Addresses where the peer can be reached.
- Supported Protocols: A list of protocols the peer supports.

Peer discovery is done using the [rendezvous protocol][3], this protocol involves a system where peers join a shared namespace to advertise themselves and find others.
When a new peer wants to participate in the network, it first connects to a bootstrap node (also known as a rendezvous point), which is a well-known and reachable peer that helps initialize connections within the network.
Once connected to the bootstrap node, the new peer uses the [rendezvous protocol][3] to discover other peers. The [rendezvous protocol][3] operates by having peers register their presence under a specific namespace with a designated rendezvous point.
Peers seeking connections to others interested in the same namespace query the rendezvous point, which returns a list of peer addresses registered under that namespace.

## Multi-Address Resolution

Once the target peer is discovered, the application will use the [identify protocol][3] to retrieve detailed information, including the peer's [multi-addresses][4], supported protocols, and additional metadata.

The [identify protocol][3] facilitates the exchange of information between peers once a connection is established. After discovering the target peer via the [rendezvous protocol][2], the application will:

- Establish a direct connection to the target peer using its discovered address.
- Initiate the [identify protocol][3] handshake to request identity information.
- Extract and display the [multi-addresses][4] and associated data provided by the target peer.

The application will output this information in a human-readable format, making it easy for clients to consume and utilize in further operations.
The combination of the [rendezvous protocol][2] and [identify protocol][3] ensures a robust and modular approach to both discovering and resolving peers in the network.

## Command-Line Interface

The application will provide an intuitive CLI interface, allowing clients to input a [Peer ID][5] and retrieve its [multi-addresses][4].
Additional options will enable customization of settings, such as specifying bootstrap nodes or adjusting discovery parameters.
The output will include a list of [multi-addresses][4] associated with the [Peer ID][5], presented in a structured and readable format.

## Conclusion

The [Peer ID][5] to [Multi-Address][5] Resolver application provides utility for interacting with [libp2p][1] networks.
By leveraging [libp2p][1]'s [rendezvous protocol][2] and [identify protocol][3], the application ensures efficient peer discovery and address resolution, fostering seamless communication in decentralized systems.

[1]: https://docs.libp2p.io/
[2]: https://github.com/libp2p/specs/blob/master/rendezvous/README.md
[3]: https://github.com/libp2p/specs/blob/master/identify/README.md
[4]: https://github.com/libp2p/specs/blob/master/addressing/README.md#multiaddr-in-libp2p
[5]: https://docs.libp2p.io/concepts/fundamentals/peers/#peer-id
