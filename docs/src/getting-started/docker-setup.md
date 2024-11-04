# Docker Setup

This guide will outline how to setup your environment using Docker to get started with the polka-storage project.

## Pre-requisites

Install docker on your system by following the [docker install instructions](https://docs.docker.com/engine/install/)

## Building

Clone the repository and go into the directory:

```shell
git clone git@github.com:eigerco/polka-storage.git
cd polka-storage
```

A Dockerfile for each binary has been created and can be found in the `docker/` folder.

To make building simple there are [Just](https://github.com/casey/just) commands setup to get you started on building docker images.

### Just building commands

| Command                  | Description                                                                                                                         |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `build-mater-docker`     | This command builds the mater CLI image which is used by storage clients to convert files to CARv2 format and extract CARv2 content |
| `build-parachain-docker` | This command builds the storage chain node image                                                                                    |
| `build-sp-client`        | This command builds the image with the binary that storage clients use to interact with the chain                                   |
| `build-sp-server`        | This command builds the image with the binary for the RPC server used by the storage provider                                       |
| `build-storagext-docker` | This command builds the storagext CLI image used to execute extrinsics.                                                             |
| `build-docker`           | This command builds all images, this might take a while to complete.                                                                |
