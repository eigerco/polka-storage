# The `system` command

The command provides various utilities for interacting with the blockchain. It allows you to retrieve information about the current state of the chain.

<div class="warning">
If you haven't done so before, you should read the <a href="./index.md"><code>storagext-cli</code> getting started</a> page,
which covers the basic flags necessary to operate the CLI.
</div>

## `get-height`

The command allows you to get the current block height of the chain.

### <a class="header" id="get-height.example" href="#get-height.example">Example</a>

Getting the current block height of the chain.

```bash
storagext-cli system get-height
```

## `wait-for-height`

The command allows you to wait for the chain to reach a specific block height. It will exit once the chain has reached the specified height.

### Parameters

| Name     | Description                  | Type             |
| -------- | ---------------------------- | ---------------- |
| `HEIGHT` | The block height to wait for | Positive integer |

### <a class="header" id="wait-for-height.example" href="#wait-for-height.example">Example</a>

Waiting for the chain to reach block height 100.

```bash
storagext-cli system wait-for-height 100
```
