# Development environment for Polka Storage Node 

## Setup

### Requirements
- [nix](https://nixos.org/download/) with [flakes](https://nixos.wiki/wiki/flakes) enabled (`echo 'experimental-features = nix-command flakes' >> ~/.config/nix/nix.conf`)
    - reasoning: every developer has the same version of development tools (rust, protoc, zombienet), directed by [flake.nix](./flake.nix)`. 
    - how it works? fasterthanli.me has [a whole series on it](https://fasterthanli.me/series/building-a-rust-service-with-nix/part-10).
    - optional: [vscode extension for Nix](https://marketplace.visualstudio.com/items?itemName=jnoortheen.nix-ide)
- [direnv](https://direnv.net/) with a [shell hook](https://direnv.net/docs/hook.html)
    - *VS Code only* [direnv extension](https://marketplace.visualstudio.com/items?itemName=mkhl.direnv) (uses the same tooling as rust-toolchain.toml defined).
    - reasoning: when you enter a directory it uses everything defined in [.envrc](./.envrc), e.g. environment variables, `nix`, secrets.

## Usage

0. [Optional, if you don't have `direnv`] `nix develop`
1. Verify:
```
$ polkadot --version
polkadot 1.11.0-0bb6249

$ cargo --version
cargo 1.77.0 (3fe68eabf 2024-02-29)

$ zombienet version
1.3.103
```
2. `just testnet`
3. Get into a direct link of `charlie` node to access parachain interface and see block production (it takes ~30s).
    - testnet is defined via [zombienet configuration](https://paritytech.github.io/zombienet/guide.html) in [local-testnet.toml](./scripts//local-testnet.toml)

## Maintenance

- Updating nix flakes (`flake.lock` file has frozen state of package): `nix flake update`.
- Running out of the disk space? `nix-collect-garbage`.