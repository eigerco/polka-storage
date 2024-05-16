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

## How it works?
Nix is a package manager, which sneakily downloads all of the dependencies and updates PATH when you launch it with `nix develop`. 
You end up having all of the required dependencies in a configured shell (so you don't have to install a specific cargo version, just, polkadot on your own).
`nix develop` needs to be used in the workspace root, as it depends on [flake.nix](./flake.nix) file.
`direnv` is a shell hook, which configures your shell based on the [.envrc](./.envrc) file. 
In our case it just launches `nix develop` shell for you and when you exit the folder, it disables it.


## Usage

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