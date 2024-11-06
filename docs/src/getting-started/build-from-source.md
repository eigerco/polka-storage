# Build from source

This guide will outline how to setup your environment on a debian based machine to get started with the polka-storage project.

## Installing dependencies

Use the `install-deps.sh` script to install the required and optional packages.
In the section below you can read what is being installed with the script.

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

If you installed [Just](https://github.com/casey/just) the following table shows which building commands are supported. If you choose not to use Just the build commands can be found in the Justfile in the root of the directory.

### Just building commands

| Command                               | Description                                                                                                                   |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `build-polka-storage-node`            | This command builds the storage chain node                                                                                    |
| `build-polka-storage-provider-client` | This command builds the binary that storage clients use to interact with the chain                                            |
| `build-polka-storage-provider-server` | This command builds the RPC server used by the storage provider                                                               |
| `build-storagext-cli`                 | This command builds the storagext CLI used to execute extrinsics                                                              |
| `build-mater-cli`                     | This command builds the mater CLI which is used by storage clients to convert files to CARv2 format and extract CARv2 content |
| `build-client-binaries`               | This command builds all the storage client binaries                                                                           |
| `build-provider-binaries`             | This command builds all the storage provider binaries                                                                         |
| `build-binaries`                      | This command builds all binaries                                                                                              |
