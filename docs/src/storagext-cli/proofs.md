# The `proofs` command

<div class="warning">
This command will be removed in the future. It's currently provided for easier testing.
<br>
The <a href="./index.md"><code>storagext-cli</code> getting started</a> page covers the basic flags necessary to operate the CLI and should be read first.
</div>

Under the `proofs` subcommand [Proofs](../architecture/pallets/proofs.md) related extrinsics are available. This chapter covers the provided commands and how to use them.

## `set-porep-verifying-key`

The `set-porep-verifying-key` adds PoRep verifying key to the chain.

### Parameters

| Name  | Description               | Type   |
| ----- | ------------------------- | ------ |
| `KEY` | Hex encoded verifying key | String |

### <a class="header" id="set-porep-verifying-key.example" href="#set-porep-verifying-key.example">Example File</a>

Adding a PoRep verifying key to the chain.

```bash
storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key @2KiB.porep.vk.scale
```
