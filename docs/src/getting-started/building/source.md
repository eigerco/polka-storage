# Building from source

This guide will outline how to setup your environment to build the Polka Storage parachain,
we cover how to build the binaries directly on your system or using [Nix](https://nixos.org/download/) to ease the process.

* [Get the code](#get-the-code)
* [System dependencies](#system-dependencies)
* [Using Nix](#using-nix)
  * [Pre-requisites](#pre-requisites)
* [Building](#building)

## Get the code

To get started, first clone the repository and enter the repository's directory:

```bash
git clone git@github.com:eigerco/polka-storage.git
cd polka-storage
```

## System dependencies

To build the binaries directly on your system you will need the following tools:

* Rust 1.81.0 — you can install it using [`rustup`](https://rustup.rs/) and its [guide](https://rust-lang.github.io/rustup/installation/other.html) for help.
* Other dependencies — keep reading, we'll get to it after the end of this list!
* `just` (optional) — (after installing Rust) you can use `cargo install just` or check the [official list of packages](https://just.systems/man/en/packages.html).

The dependencies mentioned are for Linux distros using the `apt` family of package managers.
Different systems may use different package managers, as such, they may require you to find the equivalent package.

To install the required dependencies run the following commands:

```shell
$ sudo apt update
$ sudo apt install -y libhwloc-dev \
    opencl-headers \
    ocl-icd-opencl-dev \
    protobuf-compiler \
    clang \
    build-essential \
    git \
    curl
```

<div class="warning">
Not all of the binaries can be built from the polka-storage repository!
We depend on the <a href="https://github.com/paritytech/zombienet/releases/tag/v1.3.116">zombienet</a> and <a href="https://github.com/paritytech/polkadot-sdk/releases/tag/polkadot-stable2409-2">polkadot, polkadot-prepare-worker, polkadot-execute-worker</a> binaries which need to be downloaded regardless (if not using <a href="#using-nix">Nix</a>).
</div>

## Using Nix

You can use Nix to simplify the building process,
if you're just taking the network for test-drive this is a great method to get started.

Nix will take care of setting up all the dependencies for you!
If you're curious, you can read more about using Nix in [fasterthanlime's blog](https://fasterthanli.me/series/building-a-rust-service-with-nix/part-9),
the [official Nix guide](https://nixos.org/learn/) or [Determinate Systems' Zero to Nix guide](https://zero-to-nix.com/).

<div class="warning">
Binaries built using Nix <b>will not</b> work on other systems since they will be linked with Nix specific paths.
</div>

### Pre-requisites

- `nix` — which you can install by following the [official guide](https://nixos.org/download/)
  or using the [Determinate Systems installer](https://github.com/DeterminateSystems/nix-installer) — the latter being usually more reliable on MacOS systems.
- `direnv` (optional) — which you can install by following the [official guide](https://direnv.net/docs/installation.html)

If you're using `direnv`, when going into the cloned directory for the first time `nix` will activate automatically and
install the required packages, this make take some time.

If you're _not_ using `direnv`, you will need to run `nix develop` to achieve the same effect —
for more information refer to the official Nix guide — https://nix.dev/manual/nix/2.17/command-ref/new-cli/nix3-develop.

## Building

After all this setup, it is time to start building the binaries, which you can do manually using the following command:

<div class="warning">

When building `polka-storage-node` you should add `--features polka-storage-runtime/testnet` which enables the testnet configuration; all the code in the repo is currently targeting this feature, not including it may lead to unexpected behavior.

</div>

```bash
cargo build --release -p <BINARY-NAME>
```

Where `<BINARY-NAME>` is one of:

- `polka-storage-node`
- `polka-storage-provider-client`
- `polka-storage-provider-server`
- `storagext-cli`
- `mater-cli`


For more information on what each binary does, refer to [Building](./index.md).

### Just recipes

To simplify the building process, we've written some [Just](https://github.com/casey/just) recipes.

| Command                               | Description                                                                                                         |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `build-polka-storage-node`            | Builds the Polka Storage parachain node.                                                                            |
| `build-polka-storage-provider-server` | Builds the Storage Provider server binary.                                                                          |
| `build-polka-storage-provider-client` | Builds the Storage Provider client binary.                                                                          |
| `build-storagext-cli`                 | Builds the `storagext` CLI used to execute extrinsics.                                                              |
| `build-mater-cli`                     | Builds the `mater` CLI which is used by storage clients to convert files to CARv2 format and extract CARv2 content. |
| `build-binaries-all`                  | Builds all the binaries above, this may take a while (but at least `cargo` reuses artifacts).                       |

## Running

After building the desired binaries, you can find them under the `target/release` folder
(or `target/debug` if you didn't use the `-r`/`--release` flag).

Assuming you're in the project root, you can run them with the following command:

```bash
$ target/release/<BINARY-NAME>
```

Where `<BINARY-NAME>` is one of:
* `polka-storage-node`
* `polka-storage-provider-server`
* `polka-storage-provider-client`
* `mater-cli`
* `storagext-cli`

> Additionally, you can move them to a folder under your `$PATH` and run them as you would with any other binary.
