# Mater

A Rust library to read and write CAR files.

## Specifications

CARv1 specification: https://ipld.io/specs/transport/car/carv1/
CARv2 specification: https://ipld.io/specs/transport/car/carv2/
UnixFS specification: https://github.com/ipfs/specs/blob/e4e5754ad4a4bfbb2ebe63f4c27631f573703de0/UNIXFS.md

## Developing

### Overview

This crate is composed of three main modules:

- `unixfs/` — which covers the main UnixFS abstractions
- `v1/` — which contains the CARv1 implementation and respective abstractions
- `v2/` — which contains the CARv2 implementation and respective abstractions

### Further notes

The [`unixfs_pb.rs`](src/unixfs/unixfs_pb.rs) was automatically generated using
[`pb-rs`](https://github.com/tafia/quick-protobuf/tree/master/pb-rs).
The file was generated and checked-in instead of making `pb-rs` part of the build
because the definition file ([`unixfs.proto`](src/unixfs/unixfs.proto)) does not
change frequently, hence, there is no need to add complexity to the build process.

## Acknowledgements

We'd like to thank all the people that participated in the projects mentioned in this section.
In a way or another, they were all instrumental in the implementation of the present library.

* [go-car](https://github.com/ipld/go-car) — the original implementation.
* [beetle](https://github.com/n0-computer/beetle) — the library `mater` is based on.
  We've gutted out the important bits for this project, but without it, this work would've been much harder.
* [ImHex](https://github.com/WerWolv/ImHex) — for saving hours when comparing binary files.

### Similar libraries/sources

* [Forest](https://github.com/ChainSafe/forest/blob/62e55df27a091ba7993a60cc1e72622ad8e25151/src/utils/db/car_stream.rs#L155)
* [rust-car](https://github.com/jaeaster/rust-car)
* [rs-car](https://github.com/dapplion/rs-car)
* [car-utils](https://github.com/blocklessnetwork/car-utils)
