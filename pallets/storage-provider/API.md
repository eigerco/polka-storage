# Storage Provider pallet API

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

`deadline`: The deadline index which the submission targets.
`partitions`: The partitions being proven.
`proofs`: An array of proofs, one per distinct registered proof type present in the sectors being proven.

## Declaring faults

Storage providers can use the `declare_faults` extrinsic to declare a set of sectors as 'faulty', indicating that the next PoSt for those sectors' deadline will not contain a proof for those sectors' existence.

### Arguments

`deadline`: The deadline to which the faulty sectors are assigned
`partition`: Partition index within the deadline containing the faulty sectors.
`sectors`: Sectors in the partition being declared faulty.

## Declaring faults as recovered

Storage providers can declare a set of faulty sectors as "recovering", indicating that the next PoSt for those sectors' deadline will contain a proof for those sectors' existence.

### Arguments

`deadline`: The deadline to which the recovered sectors are assigned
`partition`: Partition index within the deadline containing the recovered sectors.
`sectors`: Sectors in the partition being declared recovered.

## Pre committing sectors

The Storage Provider can use the `pre_commit_sector` extrinsic to pledge to seal and commit some new sectors.

### Arguments

`sectors`: Sectors to be committed.


## Prove commit sectors

Storage providers can use the `prove_commit_sector` extrinsic to check the state of the corresponding sector pre-commitments and verifies aggregate proof of replication of these sectors. If valid, the sectors' deals are activated.

### Arguments

`sector_number`: The sector number to be proved.
`proof`: The proof, supplied by the storage provider.
