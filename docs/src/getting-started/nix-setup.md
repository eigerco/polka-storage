# Nix Setup

This guide will outline how to setup your environment using Nix to get started with the polka-storage project.

## Pre-requisites

Install direnv on your system by following the install instructions on their website for your system.

[direnv website](https://direnv.net/docs/installation.html)

Install Nix on your system with the following command:

`sh <(curl -L https://nixos.org/nix/install)`

[NixOS website](https://nixos.org/download/)

## Building

Clone the repository and go into the directory:

```shell
git clone <public-repo-url>
cd polka-storage
```

When going into the cloned directory for the first time the required packages will be installed in the Nix environment, this make take some time.
This Nix setup only needs to be done once.

Once the Nix setup has completed we can start building the binaries.

To make building simple there are [Just](https://github.com/casey/just) commands setup to get you started.

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
