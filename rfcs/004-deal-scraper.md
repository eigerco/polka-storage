# RFC: On-chain deal scraper

## Abstract

This RFC proposes the design and functionality of an application to map content CIDs to their respective owners, identified by [Peer ID][1]s, in the Polka-Storage ecosystem.
Leveraging the market pallet and storage provider pallet within the Polka-Storage network, the application will extract, correlate, and maintain relationships between [AccountID][2]s, [Peer ID][1]s, deal IDs, and `piece_cids`.
The resulting mapping will facilitate efficient identification of deal owners and their associated data pieces in the network, providing a robust index for querying storage information.

## Motivation

In the Polka-Storage ecosystem, it is crucial to track the ownership and storage relationships of data pieces efficiently.
Existing mechanisms in the storage and market pallets provide the underlying data but lack a comprehensive and centralized mapping of these relationships.
This application addresses the need for such mapping by integrating information across pallets to maintain a database that links [Peer ID][1]s with content CIDs.
This functionality enhances the usability and scalability of the Polka-Storage network.

## Design

The application will interface with the Polka-Storage network, specifically interacting with the storage provider pallet and the market pallet.
It will check storage values and subscribe to relevant events to construct and maintain a comprehensive mapping database.

To initiate the mapping process, the application will utilize the `StorageProviders` storage value from the storage provider pallet.
This value links [AccountID][2]s to [Peer ID][1]s, which are integral to identifying storage providers in the network.
 The application will subscribe to events emitted upon the registration of new storage providers, `StorageProviderRegistered`, ensuring that the mapping is continuously updated with the latest registration data.

Once the association between [AccountID][2]s and [Peer ID][1]s is established, the application will utilize the `SectorDeals` storage value in the market pallet to match deal IDs with storage providers.
By subscribing to events emitted for new deal IDs, `DealsPublished`, the application will dynamically update its database with ongoing deal information.
This layer of mapping bridges the storage providers with their associated deals, forming the basis for the next stage.

Finally, the application will leverage the `Proposals` storage value in the market pallet to associate deal IDs with their corresponding `piece_cids` and map them to the correct storage provider account.
This step completes the mapping by correlating the data pieces (`piece_cids`) with the owners ([Peer ID][1]s) through their deals.

The database maintained by the application will serve as a centralized source of truth for the mapping of [Peer ID][1]s to `piece_cids`.
By continuously monitoring and reacting to events in the Polka-Storage network, the application ensures real-time synchronization with the underlying storage and market data.

## Conclusion

This application enhances the Polka-Storage ecosystem by providing a vital indexing functionality that maps [Peer ID][1]s to content CIDs.
By integrating data across pallets and dynamically responding to network events, it offers a reliable and scalable solution for tracking storage ownership and deal relationships.
This tool will help clients with precise and up-to-date information, strengthening the overall infrastructure of Polka-Storage.

[1]: https://docs.libp2p.io/concepts/fundamentals/peers/#peer-id
[2]: https://docs.rs/frame-system/latest/frame_system/pallet/trait.Config.html#associatedtype.AccountId
