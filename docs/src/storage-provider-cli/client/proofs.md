# `proofs`

The following subcommands are contained under `proofs`.

> These are advanced commands and only useful for demo purposes.
> This functionality is covered in the server by the [pipeline](../../architecture/polka-storage-provider-server.md#sealing-pipeline).

| Name                         | Description                                                                                                                                 |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `calculate-piece-commitment` | Calculate a piece commitment for the provided data stored at the a given path                                                               |
| `porep-params`               | Generates PoRep verifying key and proving parameters for zk-SNARK workflows (prove commit)                                                  |
| `post-params`                | Generates PoSt verifying key and proving parameters for zk-SNARK workflows (submit windowed PoSt)                                           |
| `porep`                      | Generates PoRep for a piece file. Takes a piece file (in a CARv2 archive, unpadded), puts it into a sector (temp file), seals and proves it |
| `post`                       | Creates a PoSt for a single sector                                                                                                          |

