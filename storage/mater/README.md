# Mater

A Rust library to read and write CAR files.

## Specifications

- CARv1 specification: https://ipld.io/specs/transport/car/carv1/
- CARv2 specification: https://ipld.io/specs/transport/car/carv2/
- UnixFS specification: https://github.com/ipfs/specs/blob/e4e5754ad4a4bfbb2ebe63f4c27631f573703de0/UNIXFS.md

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

### Benchmarks

- `read` benchmark checks what is the time needed to fully read a content buffer into the `BlockStore`
- `write` checks the time needed to write the CARv2 to the buffer from `BlockStore`.
- `filestore` converts a source file to the CARv2 and writes it to the output file

Execute benchmarks with `cargo bench`.

Tested on the machine with `Ryzen 9 5950X` and `64GB DDR4`.

| Bench     | Content Size | Duplicated content (%) | Median Estimate |
| --------- | ------------ | ---------------------- | --------------- |
| read      | 10 MB        | 0                      | 4.6776 ms       |
| read      | 10 MB        | 10                     | 4.5806 ms       |
| read      | 10 MB        | 20                     | 4.6977 ms       |
| read      | 10 MB        | 40                     | 4.5534 ms       |
| read      | 10 MB        | 80                     | 4.5038 ms       |
| read      | 100 MB       | 0                      | 62.419 ms       |
| read      | 100 MB       | 10                     | 60.895 ms       |
| read      | 100 MB       | 20                     | 59.461 ms       |
| read      | 100 MB       | 40                     | 55.355 ms       |
| read      | 100 MB       | 80                     | 46.792 ms       |
| read      | 1 GB         | 0                      | 632.34 ms       |
| read      | 1 GB         | 10                     | 650.01 ms       |
| read      | 1 GB         | 20                     | 631.49 ms       |
| read      | 1 GB         | 40                     | 600.01 ms       |
| read      | 1 GB         | 80                     | 505.58 ms       |
|           |              |                        |                 |
| write     | 10 MB        | 0                      | 1.6516 ms       |
| write     | 10 MB        | 10                     | 1.0342 ms       |
| write     | 10 MB        | 20                     | 875.68 µs       |
| write     | 10 MB        | 40                     | 772.26 µs       |
| write     | 10 MB        | 80                     | 354.77 µs       |
| write     | 100 MB       | 0                      | 12.689 ms       |
| write     | 100 MB       | 10                     | 10.707 ms       |
| write     | 100 MB       | 20                     | 9.4533 ms       |
| write     | 100 MB       | 40                     | 6.7805 ms       |
| write     | 100 MB       | 80                     | 6.7805 ms       |
| write     | 1 GB         | 0                      | 123.34 ms       |
| write     | 1 GB         | 10                     | 102.39 ms       |
| write     | 1 GB         | 20                     | 91.712 ms       |
| write     | 1 GB         | 40                     | 69.273 ms       |
| write     | 1 GB         | 80                     | 23.140 ms       |
|           |              |                        |                 |
| filestore | 10 MB        | 0                      | 15.145 ms       |
| filestore | 10 MB        | 10                     | 15.179 ms       |
| filestore | 10 MB        | 20                     | 15.162 ms       |
| filestore | 10 MB        | 40                     | 15.162 ms       |
| filestore | 10 MB        | 80                     | 14.836 ms       |
| filestore | 100 MB       | 0                      | 203.85 ms       |
| filestore | 100 MB       | 10                     | 210.14 ms       |
| filestore | 100 MB       | 20                     | 220.38 ms       |
| filestore | 100 MB       | 40                     | 216.34 ms       |
| filestore | 100 MB       | 80                     | 211.12 ms       |
| filestore | 1 GB         | 0                      | 1.7674 s        |
| filestore | 1 GB         | 10                     | 1.8174 s        |
| filestore | 1 GB         | 20                     | 1.8396 s        |
| filestore | 1 GB         | 40                     | 1.8496 s        |
| filestore | 1 GB         | 80                     | 1.8774 s        |

## Acknowledgements

We'd like to thank all the people that participated in the projects mentioned in this section.
In a way or another, they were all instrumental in the implementation of the present library.

- [go-car](https://github.com/ipld/go-car) — the original implementation.
- [beetle](https://github.com/n0-computer/beetle) — the library `mater` is based on.
  We've gutted out the important bits for this project, but without it, this work would've been much harder.
- [ImHex](https://github.com/WerWolv/ImHex) — for saving hours when comparing binary files.

### Similar libraries/sources

- [Forest](https://github.com/ChainSafe/forest/blob/62e55df27a091ba7993a60cc1e72622ad8e25151/src/utils/db/car_stream.rs#L155)
- [rust-car](https://github.com/jaeaster/rust-car)
- [rs-car](https://github.com/dapplion/rs-car)
- [car-utils](https://github.com/blocklessnetwork/car-utils)
