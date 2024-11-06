# Build from source

This guide will outline how to setup your environment on a debian based machine to get started with the polka-storage project.
This guide only covers how to setup the binaries used to interact with the testnet.
For setting up the testnet please refer to the [local testnet documentation](../local-testnet.md) to get started.

## Installing dependencies

The section below runs you through the steps to install all required and optional dependencies.

Run the following command to install the required packages to build the polka-storage project.

```shell
sudo apt install -y libhwloc-dev \
    opencl-headers \
    ocl-icd-opencl-dev \
    protobuf-compiler \
    clang \
    build-essential \
    git \
    curl
```

Make sure that Rust is installed on your system. Follow the instructions from [the Rust website](https://www.rust-lang.org/tools/install) to install.

Installing [Just](https://github.com/casey/just) is optional but recommended. [Just](https://github.com/casey/just) is used to make building easier but supplying a single command to build.

## Building

Clone the repository and go into the directory:

```shell
git clone git@github.com:eigerco/polka-storage.git
cd polka-storage
```


### Just building commands

To simplify the building process, we've written some [Just](https://github.com/casey/just) recipes.

| Command                               | Description                                                                                                         |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `build-polka-storage-node`            | Builds the Polka Storage parachain node.                                                                            |
| `build-polka-storage-provider-server` | Builds the Storage Provider server binary.                                                                          |
| `build-polka-storage-provider-client` | Builds the Storage Provider client binary.                                                                          |
| `build-storagext-cli`                 | Builds the `storagext` CLI used to execute extrinsics.                                                              |
| `build-mater-cli`                     | Builds the `mater` CLI which is used by storage clients to convert files to CARv2 format and extract CARv2 content. |
| `build-binaries-all`                  | Builds all the binaries above, this may take a while (but at least `cargo` reuses artifacts).                       |

