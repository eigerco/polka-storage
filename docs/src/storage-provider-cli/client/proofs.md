# `proofs`

<!-- TODO: add parameters -->

The following subcommands are contained under `proofs`.

| Name                         | Description                                                                                                                                 |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `calculate-piece-commitment` | Calculate a piece commitment for the provided data stored at the a given path                                                               |
| `porep-params`               | Generates PoRep verifying key and proving parameters for zk-SNARK workflows (prove commit)                                                  |
| `porep`                      | Generates PoRep for a piece file. Takes a piece file (in a CARv2 archive, unpadded), puts it into a sector (temp file), seals and proves it |
| `post-params`                | Generates PoSt verifying key and proving parameters for zk-SNARK workflows (submit windowed PoSt)                                           |
| `post`                       | Creates a PoSt for a single sector                                                                                                          |


# `porep-params`

# `post-params`