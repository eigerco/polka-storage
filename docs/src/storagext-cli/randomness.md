# The `randomness` command

<div class="warning">
Random value returned by testnet node is same for all blocks.
</div>

Under the `randomness` subcommand [Randomness](../pallets/randomness.md) related extrinsics are available.
This chapter covers the provided commands and how to use them.

<div class="warning">
The <a href="./index.md"><code>storagext-cli</code> getting started</a> page covers the basic flags necessary to operate the CLI and should be read first.
</div>

## `get`

The `get` fetches random value for a specific block height. The returned value is hex encoded.

### Parameters

| Name    | Description  | Type             |
| ------- | ------------ | ---------------- |
| `BLOCK` | Block height | Positive integer |

### <a class="header" id="get.example" href="#get.example">Example</a>

Fetching random value for block 100.

```bash
storagext-cli randomness get 100
```
