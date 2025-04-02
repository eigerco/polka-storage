# The `proofs` command

<div class="warning">
This command will be removed in the future. It's currently provided for easier testing.
<br>
The <a href="./index.md"><code>storagext-cli</code> getting started</a> page covers the basic flags necessary to operate the CLI and should be read first.
</div>

Under the `proofs` subcommand [Proofs](../architecture/pallets/proofs.md) related extrinsics are available. This chapter covers the provided commands and how to use them.

## Available Commands

| Command                   | Description                            |
| ------------------------- | -------------------------------------- |
| `set-porep-verifying-key` | Set Proof of Replication verifying key |
| `set-post-verifying-key`  | Set Proof of Spacetime verifying key   |

## `set-porep-verifying-key`

The `set-porep-verifying-key` adds PoRep (Proof of Replication) verifying key to the chain.

### Parameters

| Name                       | Description                                            | Type   |
| -------------------------- | ------------------------------------------------------ | ------ |
| `KEY`                      | Hex encoded verifying key or file path prefixed with @ | String |
| `--registered-proof`, `-r` | The verifying key's proof kind                         | String |

### <a class="header" id="set-porep-verifying-key.example" href="#set-porep-verifying-key.example">Example</a>

Adding a PoRep verifying key to the chain:

```bash
storagext-cli --sr25519-key "//Alice" proofs set-porep-verifying-key @8MiB.porep.vk.scale --registered-proof 8MiB
```

## `set-post-verifying-key`

The `set-post-verifying-key` adds PoSt (Proof of Spacetime) verifying key to the chain.

### Parameters

| Name                       | Description                                            | Type   |
| -------------------------- | ------------------------------------------------------ | ------ |
| `KEY`                      | Hex encoded verifying key or file path prefixed with @ | String |
| `--registered-proof`, `-r` | The verifying key's proof kind                         | String |

### <a class="header" id="set-post-verifying-key.example" href="#set-post-verifying-key.example">Example</a>

Adding a PoSt verifying key to the chain:

```bash
storagext-cli --sr25519-key "//Alice" proofs set-post-verifying-key @8MiB.post.vk.scale --registered-proof 8MiB
```
