# Storing a file

<div class="warning">
Before reading this guide, please follow the <a href="./local-testnet.md">local testnet</a> guide and <a href="./storage-provider.md">storage provider guide</a>.
You should have a working testnet and a Storage Provider running!
</div>

## Storage Provider

Charlie heard he could provide storage to people worldwide and earn some tokens,
so he decided to register as a [Storage Provider](../glossary.md).
Charlie [also adds some funds](../architecture/pallets/market.md#add_balance) so he can use them as collateral.

```bash
$ storagext-cli --sr25519-key "//Charlie" storage-provider register Charlie
[0xb637…7fd2] Storage Provider Registered: { owner: 5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y, info: Storage Provider Info: { peer_id: 3ZAB4sc5BS, window_post_proof_type: StackedDRGWindow2KiBV1P1, sector_size: SectorSize::_2KiB, window_post_partition_sectors: 2 }, proving_period_start: 61 }
$ storagext-cli --sr25519-key "//Charlie" market add-balance 12500000000
[0x809d…8f10] Balance Added: { account: 5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y, amount: 12500000000 }
```

## Storage Client
<img class="right" src="../images/polkadot.svg" alt="The Polkadot logo" style="height: 100px; padding: 4px 8px 4px;">

Alice is a [Storage User](../glossary.md#storage-user) and wants to store an image of her lovely Polkadot logo [`polkadot.svg`](../images/polkadot.svg) in the Polka Storage [parachain](../glossary.md#parachain).

Alice knows that she needs to prepare an image for storage and get its [CID](https://github.com/multiformats/cid).
To do so, she first converts it into a [CARv2 archive](https://ipld.io/specs/transport/car/carv2/) and gets the piece cid.

```bash
$ mater-cli convert -q --overwrite polkadot.svg polkadot.car
bafkreihoxd7eg2domoh2fxqae35t7ihbonyzcdzh5baevxzrzkaakevuvy
$ polka-storage-provider-client proofs commp polkadot.car
{
	"cid": "baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq",
	"size": 2048
}
```


### Proposing a deal

Afterwards, it's time to propose a deal, currently — i.e. while the network isn't live —
any deals will be accepted by Charlie (the Storage Provider).

Alice fills out the deal form according to a JSON template (`polka-logo-deal.json`):
```json
{
  "piece_cid": "baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq",
  "piece_size": 2048,
  "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
  "label": "",
  "start_block": 200,
  "end_block": 250,
  "storage_price_per_block": 500,
  "provider_collateral": 1000100200,
  "state": "Published"
}
```

* `piece_cid` — is the `cid` field from the previous step, where she calculated the piece commitment. It uniquely identifies the piece.
* `piece_size` — is the `size` field from the previous step, where she calculated the piece commitment. It is the size of the processed piece, not the original file!
* `client` — is the client's (i.e. the reader's) public key, encoded in bs58 format.
  For more information on how to generate your own keypair, read the [Polka Storage Provider CLI/`client`/`wallet`](../storage-provider-cli/client/wallet.md).
* `provider` — is the storage provider's public key, encoded in bs58 format.
  If you don't know your storage provider's public key, you can query it using `polka-storage-provider-client`'s `info` command.
* `label` — is an arbitrary string to be associated with the deal.
* `start_block` — is the deal's start block, it MUST be positive and lower than `end_block`.
* `end_block` — is the deal's end block, it must be positive and larger than `start_block`.
* `storage_price_per_block` — the storage price over the duration of a single block — e.g. if your deal is 20 blocks long, it will cost `20 * storage_price_per_block` in total.
* `provider_collateral` — the price to pay *by the storage provider* if they fail to uphold the deal.
* `state` — the deal state, only `Published` is accepted.

When the deal is ready, she proposes it:

```bash
$ polka-storage-provider-client propose-deal --rpc-server-url "http://localhost:8000" "@polka-logo-deal.json"
bagaaierab543mpropvi5mnmtptytnnlbr2j7vea7lowcugrqt7epanybw7ta
```

The storage provider replied with a CID — the CID of the deal Alice just sent — she needs to keep this CID for the next steps!

Once the server has replied with the CID, she's ready to upload the file.
This can be done with just any tool that can upload a file over HTTP.
The server supports both [multipart forms](https://curl.se/docs/httpscripting.html#file-upload-post) and [`PUT`](https://curl.se/docs/httpscripting.html#put).

```bash
curl --upload-file "polkadot.svg" "http://localhost:8001/upload/bagaaierab543mpropvi5mnmtptytnnlbr2j7vea7lowcugrqt7epanybw7ta"
baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq
```

### Publishing the deal

Before Alice publishes a deal, she must ensure that she has the necessary funds available in the market escrow, to be able to pay for the deal:

```bash
$ storagext-cli --sr25519-key "//Alice" market add-balance 25000000000
[0x6489…a2c0] Balance Added: { account: 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY, amount: 25000000000 }
```

Finally, she can publish the deal by submitting her deal proposal along with your signature to the storage provider.

To sign her deal proposal she runs:

```bash
$ polka-storage-provider-client sign-deal --sr25519-key "//Alice" @polka-logo-deal.json
```

```json
{
  "deal_proposal": {
    "piece_cid": "baga6ea4seaqabpfwrqjcwrb4pxmo2d3dyrgj24kt4vqqqcbjoph4flpj2e5lyoq",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "",
    "start_block": 200,
    "end_block": 250,
    "storage_price_per_block": 500,
    "provider_collateral": 1000100200,
    "state": "Published"
  },
  "client_signature": {
    "Sr25519": "7eb8597441711984b7352bd4a118eac57341296724c20d98a76ff8d01ee64038f6a9881e492a98c3a190e7b600a8313d72e9f0edacb3e6df6b0b4507dabb9580"
  }
}

```

All that's left is to [publish the deal](../storage-provider-cli/client/index.md#publish-deal):

```bash
polka-storage-provider-client publish-deal --rpc-server-url "http://localhost:8000" @signed-logo-deal.json
Successfully published deal of id: 0
```

On Alice's side, that's it!
