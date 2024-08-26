# Real-world use case demo

## 1. Publishing a deal

<div class="warning">
Before reading this guide, please ensure you've followed the <a href="./local-testnet.md">local testnet</a> guide and have a working testnet running!
</div>

Charlie heard that he can provide storage to people of the world and earn by doing that, so h registered as a [Storage Provider](../glossary.md).

```bash
$ storagext-cli --sr25519-key "//Charlie" storage-provider register Charlie
2024-08-26T14:18:44.280186Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::storage_provider: [0x9a07…aaec] Successfully registered Charlie, seal: StackedDRGWindow2KiBV1P1 in Storage Provider Pallet
```

Alice is a [Storage User](../glossary.md#storage-user) and Charlie is a [Storage Provider](../glossary.md#storage-provider).
Alice wants to store image of her lovely Husky (`husky.jpg`) in Polka Storage parachain.

Alice knows[^no-cid] that she needs to get a [CID](https://github.com/multiformats/cid) of the image, so she [uploaded it to the CAR server](../storage-provider-cli/storage.md#upload-a-file)
and received a CID: `bafybeihxgc67fwhdoxo2klvmsetswdmwwz3brpwwl76qizbsl6ypro6vxq`.

Alice heard somewhere[^no-sp-discovery] in the hallways of her favourite gym that Charlie is a Storage Provider.
She calls him (off-chain) and they negotiate a deal:

`husky-deal.json`
```json
[
    {
        "piece_cid": "bafybeihxgc67fwhdoxo2klvmsetswdmwwz3brpwwl76qizbsl6ypro6vxq",
        "piece_size": 1278,
        "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        "label": "My lovely Husky (husky.jpg)",
        "start_block": 25,
        "end_block": 50,
        "storage_price_per_block": 1000000000,
        "provider_collateral": 12500000000,
        "state": "Published"
    }
]
```

- `piece_cid`:  `husky.jpg`: `bafybeihxgc67fwhdoxo2klvmsetswdmwwz3brpwwl76qizbsl6ypro6vxq`
- `piece_size`: `file(husky.jpg).size()` == `1278`,
- `client`: Alice (`5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY`)
- `provider`: Charlie (`5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y`)
- `start_block`: 5 minutes from the testnet start (5 minutes == 25 blocks) == 25,
- `end_block`: `start_block` + 5 minutes == 50,
- `storage_price_per_block`: 1_000_000_000 [Plancks](../glossary.md#planck) for a block (12 sec),
- so total price for storage of her husky for 5 minutes: 25_000_000_000 Plancks
- `provider_collateral`: 12_500_000_000 Plancks, they agreed that Charlie loses this amount when he fails to deliver.

After the negotiation, they need to [invest their funds](../pallets/market.md#add_balance) and then [publish their intent](../pallets/market.md#publish_storage_deals) so it can be checked by the parachain.
So here they go:

```bash
$ storagext-cli --sr25519-key "//Alice" market add-balance 25000000000
2024-08-26T13:08:27.090149Z  INFO run{address="5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"}: storagext_cli::cmd::market: [0x034f…b800] Successfully added 25000000000 to Market Balance
$ storagext-cli --sr25519-key "//Charlie" market add-balance 12500000000
2024-08-26T13:09:51.130294Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::market: [0xdd8e…18f2] Successfully added 12500000000 to Market Balance
$ storagext-cli --sr25519-key  "//Charlie" market publish-storage-deals --client-sr25519-key  "//Alice" "@husky-deal.json"
2024-08-26T13:10:21.260228Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::market: [0xd547…161d] Successfully published storage deals
```

^[no-cid]: we have not provided a standalone command to generate CID out of file. The CAR server is a temporary showcase component.
^[no-sp-discovery]: we have not implemented Storage Provider Discovery protocol yet.

## 2. Committing a deal

After the deals have been published, the rest is up to Charlie.
If Charlie does not behave properly and do not [pre-commit](../pallets/storage-provider.md#pre_commit_sector) and [prove](../pallets/storage-provider.md#prove_commit_sector)the deal by block 25 (`start_block`),
he is going to be slashed and all of his funds (`provider_collateral`) - gone. *You can just wait for 5 minutes and observe a DealSlashed [Event](../pallets/market.md#events) being published*.
So he's better do do so.

`pre-commit-husky.json`
```json
{
    "sector_number": 1,
    "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "deal_ids": [0],
    "expiration": 75,
    "unsealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
    "seal_proof": "StackedDRG2KiBV1P1"
}
```
- [sector_number](../glossary.md#sector) is a place, where `husky.jpg` will end up being stored. Charlie decided it'll be on his 1st sector.
- `sealed_cid`, `unsealed_cid`, `seal_proof` are the magic values[^sealing-subsystem].
- `deal_ids`: [0] - a sector can contain multiple deals, but it only contains the first one ever created (id: 0),
- `expiration`: `75` -> 5 minutes after the `end_block`, so the sector expires only after the deal has been terminated.


`prove-commit-husky.json`
```json
{
    "sector_number": 1,
    "proof": "1230deadbeef"
}
```
- `proof`: hex string of bytes of the proof^[sealing-subsystem]

```bash
$ storagext-cli --sr25519-key "//Charlie" storage-provider pre-commit "@pre-commit-husky.json"
2024-08-26T14:34:41.710197Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::storage_provider: [0x28ef…801e] Successfully pre-commited sector 1.
$ storagext-cli --sr25519-key "//Charlie" storage-provider prove-commit "@prove-commit-husky.json"
2024-08-26T14:36:41.780309Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::storage_provider: [0xbef5…a305] Successfully proven sector 1.
```

^[sealing-subsystem]: sealing and proving process are WIP, so just trust us on this one and use any valid CID values (the ones we provided are fine).

## 3. Proofs and faults

`window-proof.json`
```json
{
    "deadline": 0,
    "partitions": [0],
    "proof": {
        "post_proof": "2KiB",
        "proof_bytes": "1230deadbeef"
    }
}
```

```bash
# wait until block 28? how on earth are they supposed to know that XD
$ storagext-cli --sr25519-key "//Charlie" storage-provider submit-windowed-post "@windowed-post.json"
2024-08-26T15:30:14.720225Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::storage_provider: [0xa233…1f9d] Successfully submitted proof.
```

## 4. Reaping the rewards

```bash
$ storagext-cli --sr25519-key "//Charlie" market settle-deal-payments
2024-08-26T15:33:26.820285Z  INFO run{address="5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y"}: storagext_cli::cmd::market: [0x9aa4…dcdd] Successfully settled deal payments
```

```bash
storagext-cli --sr25519-key "//Charlie" storage-provider register Charlie && storagext-cli --sr25519-key "//Alice" market add-balance 25000000000 && storagext-cli --sr25519-key "//Charlie" market add-balance 12500000000 && storagext-cli --sr25519-key  "//Charlie" market publish-storage-deals --client-sr25519-key  "//Alice" "@husky-deal.json" && storagext-cli --sr25519-key "//Charlie" storage-provider pre-commit "@pre-commit-husky.json" && storagext-cli --sr25519-key "//Charlie" storage-provider prove-commit "@prove-commit-husky.json"
```
