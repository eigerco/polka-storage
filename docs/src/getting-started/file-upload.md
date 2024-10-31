# File upload

> This guide assumes you're connecting to a running Polka Storage Provider Server

<!-- TODO: add a server setup guide -->


This guide will walk the client through the process of uploading a file to a Polka Storage Provider.

## Preparation

The first thing you have to do is ensure that you have the following binaries:

* `mater-cli` — to convert the file into a CAR archive.
* `polka-storage-provider-client` — to generate the piece commitment for the CAR file.

Once you have them you should run the following commands:

```bash
mater-cli convert -q --overwrite "<input_file_path>" "<output_file_path>"
```

* `-q` — enables quiet mode, not outputing any content.
* `--overwrite` — overwrites the output file it exists.
* `<input_file_path>` — the path to the file you will be uploading to the storage provider.
* `<output_file_path>` — the output path for the converted file, it will be used in the next step!

```bash
polka-storage-provider-client proofs commp "<output_file_path>"
```

* `<output_file_path>` — the file path to the output file from the previous step.

You should get an output similar to the following:

```json
{
  "cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
  "size": 2048
}
```

## Proposing a deal

Afterwards, it's time to propose a deal, currently — i.e. while the network isn't live —
any deals will be accepted by the storage provider.

Proposing a deal is similar to filling out a form, except the final format is in JSON!
Let's first take a look into a complete proposal:

```json
{
  "piece_cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
  "piece_size": 2048,
  "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
  "label": "",
  "start_block": 100000,
  "end_block": 100050,
  "storage_price_per_block": 500,
  "provider_collateral": 1250,
  "state": "Published"
}
```

Let's walk throught the fields:

* `piece_cid` — is the `cid` field from the previous step, where we calculated the piece commitment. It uniquely identifies the piece.
* `piece_size` — is the `size` field from the previous step, where we calculated the piece commitment. It is the size of the processed piece, not the original file!
* `client` — is the client's (i.e. the reader's) public key, encoded in bs58 format.
  For more information on how to generate your own keypair, read the [Polka Storage Provider CLI/`client`/`wallet`](../storage-provider-cli/client/wallet.md).
* `provider` — is the storage provider's public key, encoded in bs58 format.
  If you don't know your storage provider's public key, you can query it using `polka-storage-provider-client`'s `info` command.
<!-- TODO: add section on the info command -->
* `label` — is an arbitrary string to be associated with the deal.
* `start_block` — is the deal's start block, it MUST be positive and lower than `end_block`.
* `end_block` — is the deal's end block, it must be positive and larger than `start_block`.
* `storage_price_per_block` — the storage price over the duration of a single block — e.g. if your deal is 20 blocks long, it will cost `20 * storage_price_per_block` in total.
* `provider_collateral` — the price to pay *by the storage provider* if they fail to uphold the deal.
* `state` — the deal state, only `Published` is accepted.

When you have your deal ready, you can propose it using the following command:

```bash
polka-storage-provider-client client propose-deal \
  --rpc-server-url "http://<storage-provider-ip>:<port>" \
  "<your-deal>"
```

* `<your-deal>` — is the JSON for the deal, fully filled in. You can use the path to a file containing the deal by prefixing it with `@` — e.g. `@/home/user/deal.json`.
* `--rpc-server-url` — the RPC endpoint URL
  * `<storage-provider-ip>` — the Storage Provider's IP address.
  * `<port>` — the Storage Provider's port serving the uploads, by default it is port 8001, but it can be changed.

The tool will reply with a CID — the CID of the deal you just sent — you need to keep this CID for the next steps!

## Uploading a file

Once the server has replied with the CID, you're ready to upload the file we have been working with.
This can be done with just any tool that can upload a file over HTTP.
The server supports both [`multipart forms`](https://curl.se/docs/httpscripting.html#file-upload-post) and [`PUT`](https://curl.se/docs/httpscripting.html#put).

```bash
curl -X PUT -F "upload=@<your-file>" "http://<storage-provider-ip>:<port>/upload/<deal-cid>"
# or, alternatively
curl --upload-file "<your-file>" "http://<storage-provider-ip>:<port>/upload/<deal-cid>"
```

* `<your-file>` — the original file you did all the transformations over at the begining.
* `<storage-provider-ip>` — the Storage Provider's IP address.
* `<post>` — the Storage Provider's port serving the uploads, by default it is port 8001, but it can be changed.
* `<deal-cid>` — the CID you got back from the storage provider after proposing the deal.

## Publishing the deal

> Before you publish a deal, you must ensure that you have the necessary funds available in the market escrow, you can do that by running the following command:
> ```
> storagext-cli --sr25519-key "<your-key>" market add-balance <amount-in-Planck>
> ```

Finally, you can publish the deal by submitting your deal proposal along with your signature to the storage provider.

To sign your deal proposal you can run the following command:

```bash
polka-storage-provider-client client sign-deal \
    --sr25519-key "<your-key>" \
    "<your-deal-proposal>"
```

* `--sr25519-key` — your Sr25519 key to sign the deal; you can also use ECDSA and Ed25519 by using `--ecdsa-key` and `--ed25519-key`, respectively.
* `<your-deal-proposal>` — the deal proposal you're signing.

The output should be similar to the following:
```json
{
  "deal_proposal": {
    "piece_cid": "baga6ea4seaqj527iqfb2kqhy3tmpydzroiigyaie6g3txai2kc3ooyl7kgpeipi",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "",
    "start_block": 100000,
    "end_block": 100050,
    "storage_price_per_block": 500,
    "provider_collateral": 1250,
    "state": "Published"
  },
  "client_signature": {
    "Sr25519": "c835a1c5215fc017067d30a8f49df0c643233881e57d8bd7232f695e1d28c748e8872b45712dcb403e28792cd1fb2b6161053b3344d4f6664bafec77349abd80"
  }
}
```

All that's left is to publish the deal, you can do so using the following command:

```bash
polka-storage-provider-client client publish-deal \
  --rpc-server-url "http://<storage-provider-ip>:<port>" \
  <your-signed-deal>
```

* `--rpc-server-url` — the RPC endpoint URL
  * `<storage-provider-ip>` — the Storage Provider's IP address.
  * `<port>` — the Storage Provider's port serving the uploads, by default it is port 8001, but it can be changed.
* `<your-signed-deal>` — the deal you just signed.

You can read more about it in [*Polka Storage Provider CLI/`client`/`client`*](../storage-provider-cli/client/client.md).

