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
A new extrinsic will be added to the market pallet in the polka-storage chain to allow storage providers to advertise their deal parameters on-chain.
Storage clients will be able to query deal parameters of all storage providers so they can find the best deal for them.
Deal parameters could have endless options, a good starting point is for storage providers to set a lower and upper bound for storage price per block and the duration of a deal.
These parameters can be expanded to include things such as collateral bounds and different price bounds depending on the size or duration of the deal.
Suggested values should be added to documentation so that storage providers can use them as a reference to start, encouraging storage providers to use this feature.

<details open>

<summary><b>Deal Parameters</b></summary>

```json
{
    "8MiB": {
        // Price is in blocks, in the smallest unit (plancks)
        // Parsing can be added to support DOT units
        "minimum_price": 1_000_000,
        // Duration is in blocks
        // Parsing can be added support hours, day, etc.
        "duration": {
            "minimum": 5_000,
            "maximum": 5_000_000,
        }
    }
}
```

</details>

## Market Pallet

### Publishing Deal Parameters

The market provider pallet will need some changes to support deal parameters for automatic deal making.
We do not want to force storage providers to set deal parameters so we need a new extrinsic to published deal parameters for a storage provider, `publish_deal_parameters(origin: OriginFor<T>, deal_parameters: DealParameters<BalanceOf<T>, BlockNumberFor<T>>)`.
This extrinsic will be used for initial registration and to override any existing parameters that are already set.
This will be a signed extrinsic that takes in the deal parameters and published these parameters with the storage provider calling the extrinsic.

The deal parameters will be stored in a `StorageMap` where the `AccountId` is the key and the `DealParameters` is the value.

<details open>

<summary><b>Deal Parameter Types</b></summary>

```rust
struct DealDurationBound<BlockNumber> {
    lower: Option<BlockNumber>,
    upper: Option<BlockNumber>,
}

struct DealParameters<Balance, BlockNumber> {
    minimum_price: Balance,
    duration: DealDurationBound<BlockNumber>,
}

#[pallet::storage]
pub type DealParametersTable<T: Config> =
    StorageMap<
        _, 
        _, 
        T::AccountId, 
        StorageMap<_, _ SectorSize, DealParameters<BalanceOf<T>, BlockNumberFor<T>>>
    >
```

</details>

### Deal Parameter Validation

Deal parameters add additional validation to deals to check if the deals fall within the bounds of what the storage provider wants.
Currently, deals are validated in the market pallet.
This will not change when adding deal parameters, the checks will be extended to include the deal parameters.

## Storage Provider

The storage provider server and client will need some changes to accommodate supporting deal parameters.

The storage provider client will mostly stay the same, only adding an interface to retrieve deal parameters.

The storage provider server will check the proposed deal parameters, by getting them from the chain, to see if the values fall within what they accept and process deals in the same way.
To update the deal parameters during runtime a privileged RPC endpoint will be added.
Ideally, there would be a control interface for the storage provider to change these values.

<details open>

<summary><b>Deal Flow</b></summary>

![Deal Flow](<images/deal-flow.png>)

</details>

## Conclusion

Storage providers being able to set deal parameters for the deals will be a big step for this project, will increase price transparency and user experience.
Allowing both the storage clients and the storage providers to set a price will create a more open market.
The deal parameters functionality will unlock more use cases such as automatic deal making and auction houses.
