# Polka Storage pallets

- [`storage-provider`](storage-provider.md) - A pallet that manages storage providers and their associated data.
- [`market`](market.md) - A pallet that handles the storage market operations.

## Overview

The Polka Storage parachain is all about making storage deals, let us go over how a deal is done!

Before anything else, [Storage Providers](../glossary.md#storage-provider) need to register themselves with the [Storage Provider Pallet](./storage-provider.md) — they can do so using the [`register_storage_provider`](./storage-provider.md#register_storage_provider) extrinsic.

<img src="../images/storage-provider/register_storage_provider.svg" alt="Storage Provider registration">

Now that storage providers can be registered in the system, we need to add some balance to both the
[Storage User's](../glossary.md#storage-user) and the Provider's accounts,
that is done using the Market's [`add_balance`](./market.md#add_balance) extrinsic.

<img src="../images/market/add_balance.svg" alt="Adding balance to Market accounts">

Afterwards, storage users and providers negotiate data storage deals off-chain,
once a deal between the two parties is reached, the client can sign the deal and send it to the storage provider for publishing
— the storage provider will then publish the signed deal using the [`publish_storage_deals`](market.md#publish_storage_deals) extrinsic.

After publishing, the funds allocated for the deal will be moved from `free` to `locked` and they can no longer be withdrawn until the deal has ended.

<img src="../images/market/publish_storage_deals.svg" alt="Publishing storage deals">

At this point the remaining responsibility is shifted to the storage provider, which needs to activate the deal;
to do so, the storage provider first needs to [pre-commit](./storage-provider.md#pre_commit_sector) the deal's sectors,
and then [prove](./storage-provider.md#prove_commit_sector) they stored the sectors — these two steps are done using the [`pre_commit_sector`](./storage-provider.md#pre_commit_sector) and [`prove_commit_sector`](./storage-provider.md#prove_commit_sector) extrinsics.

Verification is done and reported to the Market pallet, as to terminate the deal and apply penalties to the storage provider
(remove and burn its collateral — i.e. `locked` funds) if they fail to activate the deal on time and return the funds to the client.

<img src="../images/storage-provider/sector_activation.svg" alt="Deal activation">

If the deal has been completed successfully or is **Active**, the storage provider is now required to periodically submit proofs that they're still storing the user's data
— the storage provider does this by calculating a proof and submitting it using [`submit_windowed_post`](./storage-provider.md#submit_windowed_post).

<img src="../images/storage-provider/submit_windowed_post.svg" alt="Proving the data is still stored">

Finally, storage providers can then settle deal payments to receive their fair share for keeping the user's data safe — using the [`settle_deal_payments`](./market.md#settle_deal_payments) extrinsic.

<img src="../images/market/settle_deal_payments.svg" alt="Settling deal payments">

Putting it all together, we get the following:

<img src="../images/overview_flow.svg" alt="The described flow">


