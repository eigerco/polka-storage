# Storage Provider pallet API

> [!NOTE]
> Some terms used in this document are described in the [design document](./DESIGN.md#constants--terminology)

## Creating Storage Provider

When a Storage provider first starts up it needs to index itself in the storage provider pallet. The `create_storage_provider` extrinsic is used for this.

### Arguments

`peer_id`: storage_provider's libp2p peer id in bytes.
`window_post_proof_type`: The Proof of Spacetime type the storage provider will submit.

## Change peer id

The `change_peer_id` extrinsic is used by the Storage Provider to update its peer id.

### Arguments

`peer_id`: The new peer id for the Storage Provider.

## Change owner address

The `change_owner_address` extrinsic is used by the Storage Provider to update its owner.

### Arguments

`new_owner`: The address of the new owner.

## Submitting Proof of Spacetime

The `submit_windowed_post` is used by the storage provider to submit their Proof of Spacetime

### Arguments

`deadline`: The [deadline](./DESIGN.md#constants--terminology) at which the submission targets.
`partition`: The [partition](./DESIGN.md#constants--terminology)s being proven.
`proofs`: An array of proofs, one per distinct registered proof type present in the [sectors](./DESIGN.md#constants--terminology) being proven.

## Declaring faults

Storage providers can use the `declare_faults` extrinsic to declare a set of [sectors](./DESIGN.md#constants--terminology) as 'faulty', indicating that the next PoSt for those [sectors](./DESIGN.md#constants--terminology)' [deadline](./DESIGN.md#constants--terminology) will not contain a proof for those [sectors](./DESIGN.md#constants--terminology)' existence.

### Arguments

`deadline`: The [deadline](./DESIGN.md#constants--terminology) to which the faulty [sectors](./DESIGN.md#constants--terminology) are assigned
`partition`: [Partition](./DESIGN.md#constants--terminology) index within the [deadline](./DESIGN.md#constants--terminology) containing the faulty [sectors](./DESIGN.md#constants--terminology).
`sectors`: [Sectors](./DESIGN.md#constants--terminology) in the [partition](./DESIGN.md#constants--terminology) being declared faulty.

## Declaring faults as recovered

Storage providers can declare a set of faulty [sectors](./DESIGN.md#constants--terminology) as "recovering", indicating that the next PoSt for those [sectors](./DESIGN.md#constants--terminology)' [deadline](./DESIGN.md#constants--terminology) will contain a proof for those [sectors](./DESIGN.md#constants--terminology)' existence.

### Arguments

`deadline`: The [deadline](./DESIGN.md#constants--terminology) to which the recovered [sectors](./DESIGN.md#constants--terminology) are assigned
`partition`: [Partition](./DESIGN.md#constants--terminology) index within the [deadline](./DESIGN.md#constants--terminology) containing the recovered [sectors](./DESIGN.md#constants--terminology).
`sectors`: [Sectors](./DESIGN.md#constants--terminology) in the [partition](./DESIGN.md#constants--terminology) being declared recovered.

## Pre committing [sectors](./DESIGN.md#constants--terminology)

The Storage Provider can use the `pre_commit_sector` extrinsic to pledge to seal and commit some new [sectors](./DESIGN.md#constants--terminology).

### Arguments

`sectors`: [Sectors](./DESIGN.md#constants--terminology) to be committed.

## Prove commit [sectors](./DESIGN.md#constants--terminology)

Storage providers can use the `prove_commit_sector` extrinsic to check the state of the corresponding sector pre-commitments and verifies aggregate proof of replication of these [sectors](./DESIGN.md#constants--terminology). If valid, the [sectors](./DESIGN.md#constants--terminology)' deals are activated.

### Arguments

`sector_number`: The sector number to be proved.
`proof`: The proof, supplied by the storage provider.
