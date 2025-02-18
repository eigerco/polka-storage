# RFC-005: Deal Parameters

Author: Aidan Sullivan — @aidan46
Date: 18/02/25

## Abstract

This RFC proposes features that allow storage providers to set terms for deals so they can automatically accept the deals.
This will enhance user experience on both the storage provider and storage client side by requiring less intervention on deals that fall between the storage provider's set parameters.

## Introduction

Currently, storage providers accept any deal that adheres to the minimum chain requirements for deals.
These are just basic requirements that makes sure the deal is valid such as: the start block is before the end block, a deal does not have a start block before the current block and deal durations fall between the chains set deal duration.
All these requirements make sense but do not give the storage provider any control over what is important for them, profit 💰.
Allowing the storage provider to set terms for deals they accept will increase transparency on storage deal prices and allows for storage providers to automatically accept deals.
This, in turn, will improve the storage client experience by making the price of storage more available.
Storage clients also no longer need to propose a deal and wait for it to be accepted by the storage provider.
They can propose a deal within the parameters set by the storage provider and be sure that the deal will be accepted.

## Deal Parameters

Storage providers will be able to set their deal parameters by sharing these parameters after registering as a storage provider on the polka-storage chain.
The deal parameters are not shared during registration because we do not want to force storage providers to use this feature.
A new extrinsic will be added to the storage provider pallet in the polka-storage chain to allow storage providers to advertise their deal parameters on-chain.
Storage clients will be able to query deal parameters of all storage providers so they can find the best deal for them.
Deal parameters could have endless options, a good starting point is for storage providers to set a lower and upper bound for storage price per block and the duration of a deal.
These parameters can be expanded to include things such as collateral bounds and different price bounds depending on the size of the deal.

<details open>

<summary><b>Deal Parameters</b></summary>

```json
{
    "price_per_block": {
        // Price is in the smallest unit (plancks)
        "minimum": 1_000_000,
        "maximum": null,
    },
    "duration": {
        // Duration is in block
        "minimum": 5_000,
        "maximum": 5_000_000,
    }
}
```

</details>

## Pallet Changes

### Storage Provider Pallet

The storage provider pallet will need some changes to support deal parameters for automatic deal making.
We do not want to force storage providers to set deal parameters so we need a new extrinsic to register deal parameters for a storage provider, `register_deal_parameters(origin: OriginFor<T>, deal_parameters: DealParameters<BalanceOf<T>, BlockNumberFor<T>>)`.
This extrinsic will be used for initial registration and to override any existing parameters that are already set.
This will be a signed extrinsic that takes in the deal parameters and registers these parameters with the storage provider calling the extrinsic.

The deal parameters will be stored in a `StorageMap` where the `AccountId` is the key and the `DealParameters` is the value.

<details open>

<summary><b>Deal Parameter Types</b></summary>

```rust
struct DealPriceBound<Balance> {
    lower: Balance,
    upper: Option<Balance>,
}

struct DealDurationBound<BlockNumber> {
    lower: BlockNumber,
    upper: Option<BlockNumber>,
}

struct DealParameters<Balance, BlockNumber> {
    price: DealPriceBound<Balance>,
    duration: DealDurationBound<BlockNumber>,
}

#[pallet::storage]
pub type DealParametersTable<T: Config> =
    StorageMap<_, _, T::AccountId, DealParameters<BalanceOf<T>, BlockNumberFor<T>>>;
```

</details>

### Market Pallet

Deal parameters add additional validation to deals to check if the deals fall within the bounds of what the storage provider wants.
Currently, deals are validated in the market pallet.
This will not change when adding deal parameters, the checks will be extended to include the deal parameters.

Since the storage provider pallet holds all the information about the deal parameters the simplest way to do this check would be to extend the `StorageProviderValidation` trait used by the market pallet to include a deal parameter check.

<details open>

<summary><b>StorageProviderValidation trait</b></summary>

```rust
pub trait StorageProviderValidation<AccountId, Balance, BlockNumber, OffchainSignature> {
    ..

    /// Checks that the proposed deal is within the bounds set by the storage provider
    fn validate_deal_parameters(
        storage_provider: &,
        deal: ClientDealProposal<AccountId, Balance, BlockNumber, OffchainSignature>
    ) -> bool;
}
```

</details>

## Storage Provider

The storage provider server and client will need some changes to accommodate supporting deal parameters.

The storage provider client will mostly stay the same, only adding an interface to retrieve deal parameters.

The storage provider server will check the proposed deal parameters to see if the values fall within what they accept and process deals in the same way.
New config values will need to be added to the server so that the storage provider can supply the deal parameters.
These values should align with what is registered on-chain.
This means that whenever the storage provider server is (re)started, it should check that the given deal parameters align with what is stored on-chain and update the values if this is not the case.

<details open>

<summary><b>Deal Flow</b></summary>

![Deal Flow](<images/deal-flow.png>)

</details>

## Conclusion

Storage providers being able to set deal parameters for the deals will be a big step for this project, will increase price transparency and user experience.
Allowing both the storage clients and the storage providers to set a price will create a more open market.
The deal parameters functionality will unlock more use cases such as automatic deal making and auction houses.
