# RFC: Polka-index

## Abstract

This document describes Polka-index, a indexing application that maps Content Identifiers (CIDs) to the Peer IDs of storage providers in a peer-to-peer (P2P) network. Users interact with Polka-index through a simple HTTP API, while Polka-Index communicates with storage providers over the P2P network using [libp2p](https://docs.libp2p.io/)'s [pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md) and [identify](https://github.com/libp2p/specs/blob/master/identify/README.md) protocols. This document outlines the system's architecture, key components, and communication protocols.

## 1. Introduction

In the polka-storage system it can be hard to find which CID is owned by which storage provider. Polka-index solves this problem by connecting CIDs (unique identifiers for files) to the Peer IDs of storage providers who store them.

Storage providers store files outside the scope of the P2P network. Instead, they use the network to broadcast their Peer IDs and respond to CID queries from the Polka-Index. Users can easily request CID-to-Peer ID mappings from the Polka-Index via HTTP.

This document explains how the system works, detailing how users, the Polka-Index, and storage providers interact in the network.

## Terminology

- **CID (Content Identifier)**: A unique identifier for a file stored by a storage provider.
- **Peer ID**: A unique identifier for a node in the P2P network.
- **Storage Provider**: A node in the network that stores files and manages a database of CIDs but uses the P2P network only to share Peer IDs and provide CID mappings.
- **Deal Database**: A local database in each storage provider where CIDs and metadata about stored files are recorded.
- **Polka-Index**: The node responsible for querying storage providers and maintaining a database of CID-to-Peer ID mappings. It also provides an HTTP API for users.
- **[pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md)**: A protocol where peers congregate around topics they are interested in.
- **[identify](https://github.com/libp2p/specs/blob/master/identify/README.md)**: A protocol used to exchange basic information with other peers in the network, including addresses, public keys, and capabilities.

## 3. System Overview

### 3.1 Components

#### Storage Providers Component

- Connect to the P2P network using unique Peer IDs.
- Broadcast their Peer IDs using the [identify](https://github.com/libp2p/specs/blob/master/identify/README.md) protocol so they can be discovered by the Polka-Index.
- Maintain a Deal Database containing the CIDs and metadata for the files they store.
- Respond to Polka-Index queries via [pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md) topics.

#### Polka-Index Component

- Connects to the P2P network using its own unique Peer ID.
- Detects storage providers through the [identify protocol](https://github.com/libp2p/specs/blob/master/identify/README.md).
- Requests unknown CIDs using the [libp2p's pubsub protocol](https://github.com/libp2p/specs/blob/master/pubsub/README.md).
- Maintains a CID-to-Peer ID mapping database.
- Provides an HTTP API for users to request CID-to-Peer ID mappings.

#### P2P Network

- A communication layer powered by [libp2p](https://docs.libp2p.io/) that supports Peer ID discovery (via [libp2p's identify protocol](https://github.com/libp2p/specs/blob/master/identify/README.md)) and subscribe to topics (via [libp2p pubsub protocol](https://github.com/libp2p/specs/blob/master/pubsub/README.md)).
Users
- Connect to the Polka-Index via HTTP to find out which Peer ID is associated with a specific CID.

### 3.2 How It Works

1. Storage providers connect to the P2P network and broadcast their Peer IDs and CIDs using pubsub.
2. Polka-Index listens for these broadcasts and keeps track of active storage providers.
3. Users send an HTTP request to the Polka-Index, asking for the Peer ID associated with a specific CID.
4. If the CID isn’t already in its database, the Polka-Index broadcasts a [pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md) message asking which storage provider owns said CID.
5. The storage provider responds with the CID-to-Peer ID mapping.
6. Polka-Index stores the mapping for future use and returns the Peer ID to the user.

### 3.3 Architecture Diagram

The following diagram illustrates the architecture of Polka-index:

![architecture](assets/Polka-indexer.svg)

### 4. Architectural Details

#### Storage Providers Architecture

- **Peer ID**: Each storage provider generates a Peer ID before it connects to the P2P network. This ID is broadcast via identify protocol for discovery.
- **Deal Database**: A lightweight database where storage providers keep records of CIDs and file metadata. File storage itself is external to the P2P network.
- **[pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md)**: Responds to direct queries from the Polka-Index to provide CID-to-Peer ID mappings.

#### Polka-Index Architecture

- **Peer ID**: The Polka-Index has its own Peer ID for identifying itself in the P2P network.
- **[identify](https://github.com/libp2p/specs/blob/master/identify/README.md) Discovery**: It listens for Peer ID broadcasts to discover active storage providers.
- **CID Mapping Database**: Stores CID-to-Peer ID mappings retrieved from storage providers. This database powers the JSON API for user queries.
- **JSON API**: Provides a simple interface for users:

### 5. Communication Protocols

#### [pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md) Protocol

- **Broadcast**: When a storage provider connects to the network, it broadcasts its owned CIDs using the pubsub topics.

#### [identify](https://github.com/libp2p/specs/blob/master/identify/README.md) Protocol

- **Discovery**: When a storage provider connects to the network, it broadcasts its peer ID using the identify protocol.

#### Query Protocol (P2P Network)

- **Request**: Polka-Index requests the owner of a CID through the [pubsub](https://github.com/libp2p/specs/blob/master/pubsub/README.md) protocol.
- **Response**: The storage provider returns the CID and its associated Peer ID.

#### User Query Protocol (HTTP)

- **Request**: Users query the Polka-Index using a simple HTTP GET request. Example:

```json
{ "cid": "<CID>" }
```

- **Response**: Polka-Index responds with the mapping. Example:

```json
{
    "cid": "<CID>",
    "peer_id": "<PeerID>"
}
```

### 6. Scalability and Performance

Polka-index is designed to scale across a growing number of storage providers and users:

- [identify](https://github.com/libp2p/specs/blob/master/identify/README.md) Discovery: Enables seamless detection of storage providers as they join or leave the network.
- Dialing: Ensures efficient, direct communication between the Polka-Index and storage providers.
- HTTP API: Allows users to interact with the system easily, with minimal overhead.

### 7. References

[libp2p Documentation](https://docs.libp2p.io/)
