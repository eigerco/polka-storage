# Development environment for Polka Storage Node 

- recommended OS: Linux.

## Setup

### Requirements
- [nix](https://nixos.org/download/) with [flakes](https://nixos.wiki/wiki/flakes) enabled (`echo 'experimental-features = nic-command flakes' >> ~/.config/nix/nix.conf`)
    - reasoning: every developer has the same version of development tools (rust, protoc, zombienet), directed by `flake.nix`. 
    - how it works? fasterthanli.me has [a whole series on it](https://fasterthanli.me/series/building-a-rust-service-with-nix/part-10).
- [direnv](https://direnv.net/) with a [shell hook](https://direnv.net/docs/hook.html)
    - installation: `nix profile install nixpkgs#direnv`
    - *VS Code only* [direnv extension](https://marketplace.visualstudio.com/items?itemName=mkhl.direnv) (uses the same tooling as rust-toolchain.toml defined).
    - reasoning: when you enter a directory it uses everything defined in `.envrc`, e.g. environment variables, `nix`, secrets.

## Usage

0. [Optional, if you don't have `direnv`] `nix develop`
1. Verify:
```
$ polkadot --version
polkadot 1.11.0-0bb6249

$ cargo --version
cargo 1.77.0 (3fe68eabf 2024-02-29)
```
2. `cargo build --release`
3. `zombienet -p native spawn scripts/local-testnet.toml`

## Maintenance

- Updating nix flakes (`flake.lock` file): `nix flake update`.