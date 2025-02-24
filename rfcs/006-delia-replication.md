# RFC-006: Automated Multi-Deal Submission in Delia for Replication

**Author:** Pete — @pete-eiger  
**Date:** 2025-02-24

## Abstract

This RFC proposes modifications to the Delia storage deal flow so that, instead of submitting a single deal proposal, the client automatically submits N number of identical deal proposals. By looping through the proposal process N times and executing the necessary RPC calls in parallel, the client not only streamlines the storage deal process but also increases data replication in the protocol. This replication will be a core feature of the Polka Storage solution, ensuring that data is redundantly stored across multiple providers for improved availability, durability, and resilience in a decentralized environment.

## Introduction

Polka Storage will rely on replicating data across multiple storage providers to ensure high availability and fault tolerance. Currently, Delia handles storage deals on a one-to-one basis: a client calculates a piece CID, proposes a deal, uploads a file, and then publishes the deal. For true replication, it is desirable for a client to automatically generate multiple identical deals so that the same piece of data is stored redundantly.

This RFC outlines the necessary modifications to the client-side deal flow to support the automatic submission of three deals. The changes will involve:

- Looping through the existing deal proposal flow three times.
- Making parallel JSON‑RPC calls against individual storage provider servers.
- Aggregating status feedback in the user interface and implementing a retry mechanism.
- Provider selection process update - instead of the user manually selecting one provider, Delia will automatically select the N (user defined number) cheapest storage providers (by price per block). Users will still have the option to replace any of them in the UI. They will also have the option of setting N to be anywhere between 1 to the total number of available storage providers.

## Motivation

- **Replication:**  
  Ensuring that data is stored in multiple locations is a cornerstone of a decentralized storage system. Automatically submitting N identical deals increases redundancy and resilience, so that if one deal or provider fails, the data remains available elsewhere.

- **User Experience:**  
  Clients gain higher confidence in data durability when the system automatically issues multiple proposals, knowing that their data will be replicated across the network.

## Proposed Changes

### Client Modifications

1. **Propose Deal (N Times):**  
   Instead of a single proposal, the client will loop through the proposal step N times. For each iteration:
   - Construct an identical deal proposal (using the calculated piece CID, client address, and other necessary parameters).
   - Send an RPC request (e.g. via `v0_propose_deal`) to the storage provider’s RPC API.
   - Execute the calls in parallel since each storage provider server operates independently.

2. **Upload File:**  
   For each deal CID returned from the proposal call, the client uploads the file via an HTTP PUT request to the corresponding endpoint (e.g. `/upload/{dealCid}`).

3. **Publish Deal:**  
   After file upload, the client:
   - Encodes the deal proposal via the storage provider server’s HTTP endpoint (e.g. `/encode_proposal`).
   - Uses the Polkadot extension to sign the encoded proposal.
   - Publishes the deal via a JSON‑RPC call (e.g. `v0_publish_deal`).

### Provider Selection

Currently, the FE allows the user to select one storage provider from a list. With the multi-deal approach for replication, we will change that to let Delia automatically pick the N cheapest storage providers.

### Storage Provider Considerations

- **Server-Side Processing:**  
  The storage provider server’s RPC endpoints remain unchanged. Since each storage provider runs its own server, the same client can submit N proposals without conflict.
  
- **On-Server Validation:**  
  The storage provider’s automated acceptance parameters (as per RFC‑005) will determine which deals are accepted.

### UX and Feedback

- **Status Aggregation:**  
  The FE should display the status of all proposals (either aggregated or individually) so that the user is aware of the overall replication progress.
- **Error Handling:**  
  If one proposal fails while the others succeed, the client should clearly report the error and support a retry mechanism where it retries the failed storage provider 3 times, if all of them fail, Delia selects another storage provider (selecting the next one in line based on price), and repeats the process, aiming to ensure that the data is stored by N storage providers.

### Documentation

- Update the Delia documentation to reflect the new multi-deal submission process with an emphasis on replication.
- Include examples showing how a client initiates N simultaneous proposals to achieve redundancy.
- Document any new UI changes related to replication.

## Implementation Details

- **Looping Mechanism:**  
  Modify the existing deal proposal code to iterate N times. Each iteration constructs the same deal proposal and calls the RPC endpoint.
  
- **Error Handling:**  
  Aggregate results from the three RPC calls. If there is a failed upload attempt, Delia will retry 3 times (for each failed provider), if all 3 are unsuccessful it will select another storage provider (the next one in line based on price) and repeat this, with the goal of ensuring that the data is stored by N different storage providers.
