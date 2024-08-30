# The `wallet` command

The wallet command is a re-export of the Substrate CLI,
for detailed documentation you can check the following links:

- [`sc_cli`](https://docs.rs/sc-cli/0.46.0/sc_cli/commands/index.html)
- [`subkey` subcommands](https://docs.substrate.io/reference/command-line-tools/subkey/#subcommands)

## Subcommands

You can use the following commands with the `wallet` subcommand:

| Command             | Description                                                                                            |
| ------------------- | ------------------------------------------------------------------------------------------------------ |
| `generate-node-key` | Generate a random node key, write it to a file or stdout and write the corresponding peer-id to stderr |
| `generate`          | Generate a random account                                                                              |
| `inspect`           | Gets a public key and a SS58 address from the provided Secret URI                                      |
| `inspect-node-key`  | Load a node key from a file or stdin and print the corresponding peer-id                               |
| `sign`              | Sign a message, with a given (secret) key                                                              |
| `vanity`            | Generate a seed that provides a vanity address                                                         |
| `verify`            | Verify a signature for a message, provided on STDIN, with a given (public or secret) key               |
| `help`              | Print this message or the help of the given subcommand(s)                                              |

## Examples

<div class="warning">
Keys shown in this page are, by default, not secure! Do not use them in production!
</div>

Generate a new random key to interact with the Polka Storage parachain:

```bash
> polka-storage-provider wallet generate
Secret phrase:       offer payment boost boy manage car asset lock cousin mountain vehicle setup
  Network ID:        substrate
  Secret seed:       0xfe36ee692552b0ce54de06ce4f5cc152fe2fa808cb40f58c81168bc1237208bb
  Public key (hex):  0x3ae6bdc05a6657cea011084d32b9970891be5d02b2101bbad0ca95d287f0226e
  Account ID:        0x3ae6bdc05a6657cea011084d32b9970891be5d02b2101bbad0ca95d287f0226e
  Public key (SS58): 5DPwBLBRGunws9T2aF59cht37HeBg9aSTAc6Fh2aFBJPSsr6
  SS58 Address:      5DPwBLBRGunws9T2aF59cht37HeBg9aSTAc6Fh2aFBJPSsr6
```

If you want to add parameters like a password, you may do so using the `--password-interactive` flag:

```bash
> polka-storage-provider wallet generate --password-interactive
Key password: <top secret hidden password>
Secret phrase:       comfort distance rack number assist nasty young universe lamp advice neglect ladder
  Network ID:        substrate
  Secret seed:       0x4243f3f1d78beb5c0408bbaeae58845881b638060380437967482be2d4d42bce
  Public key (hex):  0x3acb66c0313d0e8ef896bc2317545582c1f0a928f402bcbe4cdf6f37489ddb16
  Account ID:        0x3acb66c0313d0e8ef896bc2317545582c1f0a928f402bcbe4cdf6f37489ddb16
  Public key (SS58): 5DPo4H1oPAQwReNVMi9XckSkvW4me1kJoageggJSMDF2EzjZ
  SS58 Address:      5DPo4H1oPAQwReNVMi9XckSkvW4me1kJoageggJSMDF2EzjZ
```

Or you can just pass it before-hand:

```bash
> polka-storage-provider wallet generate --password <top secret password>
Secret phrase:       cactus art crime burden hope also thought asset lake only cheese obtain
  Network ID:        substrate
  Secret seed:       0xb69c2d238fa7641f0d69911ca8f107f1b97a51cfc71e8a06e0ec9c7329d69ff7
  Public key (hex):  0xb60a716e488bcb2a54ef1b1cf8874569d2d927cc830ae0ae1cc2612fac27f55d
  Account ID:        0xb60a716e488bcb2a54ef1b1cf8874569d2d927cc830ae0ae1cc2612fac27f55d
  Public key (SS58): 5GBPg51VZG8PobmkLNSn9vDkNvoBXV5vCGhbetifgxwjPKAg
  SS58 Address:      5GBPg51VZG8PobmkLNSn9vDkNvoBXV5vCGhbetifgxwjPKAg
```
