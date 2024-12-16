# RFC-002 Data Retrieval

Author: Rok Černič — @cernicc
Date: 16/12/24

This document aggregates research done on the topic of retrieving stored data
from the storage providers. It does not cover the retrieval markets and assumes
that no additional incentives are needed for being part of the retrieval network
and that every storage provider also provides a retrieval service. It also
covers the main protocols for transfer — Bitswap and GraphSync — recommending
the implementation of Bitswap as a first instance due to the lower upfront
effort required.

## Problem statement

We would like to support 3rd parties to retrieve data from the storage
providers. To retrieve the data stored, they should provide payload CID (root
node of the CAR file).

## Storage provider server

The current server implementation exposes an endpoint which allows 3rd parties
to upload some content. The content is then encoded as a CAR file; the file is
then packed into the unsealed sector, stored on disk; the unsealed sector is
sealed and stored on disk; the sealed sector is unreadable, as such for a
purposes of data retrieval, we either keep the unsealed sector around at the
cost of storage capacity or unseal sectors on-demand — for the purposes of POC
we expect that the unsealed sector is always available.

### Local Index Directory

As part of the storage server there is a need for a local index directory
subsystem. Sectors are opaque, meaning they don't contain metadata that
indicates where its files start and end, that is where the index enters, mapping
the sector and enabling retrieval of individual files.

### Storage provider server - retrieval provider

The retrieval process requires access to the local index and the unsealed
sectors, thus, it must be co-located with the storage process.

#### Bitswap

Bitswap is used to exchange blocks of data between peers. In short, it works on
a "question and answer" basis, where the client request the data for a given CID
and the server replies with that data, be it more CIDs or an actual block of
data. When coupled with IPLD graphs, this approach becomes "chatty" for large
files; since the first rounds of the protocol will usually consist of requesting
a CID and getting N CIDs back, requesting each of those CIDs and getting more
back, until reaching actual data blocks.

#### GraphSync

It is used to synchronize graphs across peers. It uses IPLD selector to
efficiently transfer graphs (or selections of parts of graphs) with a minimal
number of independent requests. It supersedes the Bitswap, because the client
only needs to send a single query request. The server then knows that the block
is part of some tree and returns all relative blocks back to the client.

## Retrieval client

The retrieval clients enable 3rd parties to easily retrieve stored data. Good
example of the retrieval client is Lassie which can be used as a CLI, library or
HTTP server.

When used, the client temporarily becomes a node in the same network as the
retrieval provider above. The client first queries the indexer (do not confuse
with the Local Index Directory) for retrieval candidates (storage providers).
After it receives 1 or more candidates it sends a retrieval request to those
providers. The request is done over the P2P network using Graphsync or Bitswap.
It depends on the protocol which the provider supports.

## Conclusion

This document covered the key technical challenge when implementing retrievals
for a system like Polka Storage, as well as the main contenders for the
retrieval protocol — Bitswap and GraphSync.

Both protocols have their strengths:

1. Bitswap is widely implemented, including in Rust, making it a practical
   choice for quick implementation in a proof-of-concept (POC).
2. GraphSync offers more efficient graph synchronization and selective data
   retrieval, which could be beneficial for larger files, but currently there is
   no good enough implementation in Rust.

For the immediate future and POC development, implementing Bitswap appears to be
the most pragmatic approach due to its existing Rust implementation and
straightforward nature. However, as the system evolves and scales, it may be
worthwhile to consider implementing GraphSync for its advanced features and
efficiency.

## References

- https://boost.filecoin.io/deployment/local-index-directory
- https://docs.ipfs.tech/concepts/bitswap/
- https://ipld.io/specs/transport/graphsync/
- https://github.com/ipld/ipld/blob/master/specs/transport/graphsync/index.md
- https://www.youtube.com/watch?v=tpqXUmokFZ0
- https://github.com/filecoin-project/lassie
