# Polka data retrieval

This document aggregates research done on the topic of retrieving stored data
from the storage providers. We didn't research the retrieval markets here and we
assume that no additional incentives are needed for being part of the retrieval
network. We assume that every storage provider also provides a retrieval
service.

### Use case

We would like to support 3rd parties to retrieve data from the storage
providers. To retrieve the data stored, they should provide CID (root node of
the CAR file).

## Storage provider server

The current server implementation exposes the endpoint which allows 3rd parties
to upload some content. That content is then encoded as a CAR file. That files
are then packed into the unsealed sector which is stored on disk. The unsealed
sector is sealed and also stored on disk. The sealed sector is _unreadable_ that
is why for a purposes of data retrieval we also need an unsealed sector. For the
purposes of POC we expect that the unsealed sector is always available.

### Local Index Directory

As part of the storage server there is a need for a local index directory
subsystem. The reason is because the unsealed sector doesn't contain any
information about location of the CAR files. Without the indexer, the client
would be able to retrieve only entire sectors. The indexer would allow us to
target the retrieval of specific CAR file or even a specific blocks in the CAR
file.

More resources:

- https://boost.filecoin.io/deployment/local-index-directory

### Storage provider server - retrieval provider

The process which provides retrievals should have access to the local index and
also to the unsealed sectors data. So we expect the process to be running on the
same machine or have access to the same storage device used by the provider. The
local index is used to find the sector which contains the requested data and
where exactly is that data located in the sector.

Each retrieval provider spins of the p2p network used to share data with clients.
Clients then connect to a specific retrieval provider and request the data.

#### Bitswap

The protocol is used to exchange blocks of data between peers. The downside is
that the whole CAR file needs to be retrived as a single block. Usually the
protocol is noisy because it uses a lot of small requests to ask the peers which
blocks they have. In our scenario this is not a problem because the connected
retrieval provider always have the file that the client was looking for.

- https://docs.ipfs.tech/concepts/bitswap/

#### GraphSync

It is used to synchronize graphs across peers. It uses IPLD selector to
efficiently transfer graphs (or selections of parts of graphs) with a minimal
number of independent requests.

- https://ipld.io/specs/transport/graphsync/
- https://github.com/ipld/ipld/blob/master/specs/transport/graphsync/index.md
- https://www.youtube.com/watch?v=tpqXUmokFZ0

## Retrieval client

The retrieval clients enable 3rd parties to easily retrieve stored data. Good
example of the retrieval client is Lassie. It can be used as a cli, library or
http server.

When used, the client temporarily becomes a node in the same network as the
retrieval provider above. The client first queries the indexer (The one that
Aidan is working on. Do not confuse with the Local Index Directory) for
retrieval candidates (storage providers). After it receives 1 or more candidates
it sends a retrieval request to those providers. The request is done
over the p2p network using Graphsync or Bitswap. It depends on the protocol
which the provider supports.

More resouces:

- https://github.com/filecoin-project/lassie
