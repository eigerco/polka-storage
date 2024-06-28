# Payment Channel Pallet

- [Payment Channel Pallet](#payment-channel-pallet)
    - [Overview](#overview)
    - [Data Structures](#data-structures)
        - [ChannelState](#channelstate)
        - [ChannelStatus](#channelstatus)
        - [SignedVoucher](#signedvoucher)
        - [UpdateChannelStateParams](#updatechannelstateparams)
        - [ModVerifyParams](#modverifyparams)
        - [LaneState](#lanestate)
        - [Merge](#merge)
    - [Payment Channel Flow](#payment-channel-flow)
        - [Opening](#opening)
        - [Updating Channel State](#updating-channel-state)
        - [Settling](#settling)
        - [Collecting](#collecting)

## Overview

The `Payment Channel Pallet` facilitates off-chain transactions between two parties, allowing them to establish, manage,
and settle payments securely without relying on on-chain transactions for each payment.

## Data Structures

### ChannelState

The `ChannelState` struct encapsulates the current state of a payment channel, including balances and state transition
information.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct ChannelState<AccountId, Balance> {
    /// The owner of the channel.
    pub from: AccountId,
    /// The recipient of payouts from the channel.
    pub to: AccountId,
    /// Amount successfully redeemed through the payment channel, paid out on `Collect`.
    pub to_send: Balance,
    /// Height at which the channel can be collected.
    pub settling_at: ChainEpoch,
    /// Height before which the channel `ToSend` cannot be collected.
    pub min_settle_height: ChainEpoch,
    /// Collection of lane states for the channel.
    pub lane_states: Vec<LaneState<Balance>>,
}
```

### ChannelStatus

Enumeration to represent the status of a payment channel.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum ChannelStatus {
    /// The channel is open and can be used for transactions.
    Open,
    /// The channel is in the process of settling.
    Settling,
    /// The channel has been closed and funds have been distributed.
    Closed,
}
```

### SignedVoucher

Represents a signed voucher.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SignedVoucher<AccountId, Balance> {
    /// The address of the payment channel this signed voucher is valid for.
    pub channel_addr: AccountId,
    /// Min epoch before which the voucher cannot be redeemed.
    pub time_lock_min: ChainEpoch,
    /// Maximum time lock.
    pub time_lock_max: ChainEpoch,
    /// (optional) Used by `to` to validate.
    pub secret_pre_image: Vec<u8>,
    /// (optional) Specified by `from` to add a verification method to the voucher.
    pub extra: Option<ModVerifyParams<AccountId>>,
    /// Specifies which lane the Voucher merges into (will be created if does not exist).
    pub lane: u64,
    /// Set by `from` to prevent redemption of stale vouchers on a lane.
    pub nonce: u64,
    /// Amount to be paid.
    pub amount: Balance,
    /// (optional) Can extend channel min_settle_height if needed
    pub min_settle_height: ChainEpoch,
    /// (optional) Set of lanes to be merged into `lane`.
    pub merges: Vec<Merge>,
    /// Sender's signature over the voucher (sign on none).
    pub signature: Option<Vec<u8>>,
}
```

### UpdateChannelStateParams

Structure to represent the parameters for updating the channel state.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct UpdateChannelStateParams<AccountId, Balance> {
    /// The signed voucher.
    pub sv: SignedVoucher<AccountId, Balance>,
    /// The secret for the voucher.
    pub secret: Vec<u8>,
}
```

### ModVerifyParams

Structure to represent the additional parameters for verification.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct ModVerifyParams<AccountId> {
    /// The account involved in the verification process.
    pub verifier: AccountId,
    /// The method used for verification.
    pub method: u64,
    /// The data for verification.
    pub data: Vec<u8>,
}
```

### LaneState

Structure to represent the state of a lane.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct LaneState<Balance> {
    /// The amount redeemed in the lane.
    pub redeemed: Balance,
    /// The nonce of the lane.
    pub nonce: u64,
}
```

### Merge

Structure to represent a merge operation within a signed voucher.

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Merge {
    /// The lane to be merged.
    pub lane: u64,
    /// The nonce of the lane to be merged.
    pub nonce: u64,
}
```

## Payment Channel Flow

### Opening

1. Participants agree on channel terms off-chain.
2. Call open_channel to initialize the channel on-chain with agreed parameters including the initial deposit.
3. The `open_channel` extrinsic reserves the initial deposit from the sender’s account and creates a new channel state
   with the initial parameters.
4. The channel state is stored, and an event `ChannelOpened` is emitted.

### Updating Channel State

1. Participants sign a voucher off-chain for a specific lane in the channel.
2. Call `update_channel_state` with the voucher and secret to update the channel state on-chain.
3. The function validates the caller to ensure they are either the sender or the recipient of the channel.
4. The function verifies the voucher signature against the signer’s public key.
5. The function checks the validity of the voucher against constraints like time locks and ensures the secret is correct
   if provided.
6. The function updates the lane state by merging lanes if specified in the voucher.
7. The function prevents double counting by adjusting already redeemed amounts.
8. The function calculates the new balance to be sent and ensures it does not exceed available funds or result in a
   negative balance.
9. The function updates the `settling_at` and `min_settle_height` parameters of the channel if delayed by the voucher.
10. The updated channel state is stored, and an event `ChannelUpdated` is emitted.

### Settling

1. Any participant can initiate the settlement process by calling `settle_channel`.
2. The `settle_channel` extrinsic sets the `settling_at` epoch and ensures it respects the `min_settle_height`.
3. The updated channel state is stored, and an event `ChannelSettled` is emitted.

### Collecting

1. Any participant can finalize the collection of funds from the channel by calling `collect_channel`.
2. The collect_channel extrinsic validates the caller and the state of the channel to ensure it is ready for collection.
3. The function transfers the `to_send` amount to the recipient and unreserves the remaining balance in the sender’s
   account.
4. The channel state is removed from storage, and an event `ChannelCollected` is emitted.
