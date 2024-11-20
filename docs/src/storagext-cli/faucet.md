# The `faucet` command

Under the `faucet` subcommand [faucet](../architecture/pallets/faucet.md) related extrinsics are available.
This chapter covers the provided commands and how to use them.

## `drip`

The `drip` command tops up the provided account.

### Parameters

| Name      | Description           | Type    |
| --------- | --------------------- | ------- |
| `ACCOUNT` | Account ID to drip to | Account |

### <a class="header" id="drip.example" href="#drip.example">Example</a>

Topping up `5GpRRVXgPSoKVmUzyinpJPiCjfn98DsuuHgMV2f9s5NCzG19`

```bash
storagext-cli faucet drip 5GpRRVXgPSoKVmUzyinpJPiCjfn98DsuuHgMV2f9s5NCzG19
```
