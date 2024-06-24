### How to run

Execute the benchmarks with `cargo bench`.

### Results

The benchmarks below use Median times after 100 runs. The duplication
percentages show the proportion of duplicated content in the file. The
benchmarks were performed on a machine with a `Ryzen 9 5950X` processor and
`64GB DDR4` memory.

#### read

Benchmark checks what is the time needed to fully read a content buffer into the `BlockStore`.

| Size / Duplication | 0%        | 10%       | 20%       | 40%       | 80%       |
| ------------------ | --------- | --------- | --------- | --------- | --------- |
| 10 MB              | 4.6776 ms | 4.5806 ms | 4.6977 ms | 4.5534 ms | 4.5038 ms |
| 100 MB             | 62.419 ms | 60.895 ms | 59.461 ms | 55.355 ms | 46.792 ms |
| 1 GB               | 632.34 ms | 650.01 ms | 631.49 ms | 600.01 ms | 505.58 ms |

#### write

Checks the time needed to write the CARv2 to the buffer from `BlockStore`.

| Size / Duplication | 0%        | 10%       | 20%       | 40%       | 80%       |
| ------------------ | --------- | --------- | --------- | --------- | --------- |
| 10 MB              | 1.6516 ms | 1.0342 ms | 875.68 µs | 772.26 µs | 354.77 µs |
| 100 MB             | 12.689 ms | 10.707 ms | 9.4533 ms | 6.7805 ms | 1.7487 ms |
| 1 GB               | 123.34 ms | 102.39 ms | 91.712 ms | 69.273 ms | 23.140 ms |

#### filestore

Converts a source file to the CARv2 and writes it to the output file.

| Size / Duplication | 0%        | 10%       | 20%       | 40%       | 80%       |
| ------------------ | --------- | --------- | --------- | --------- | --------- |
| 10 MB              | 15.145 ms | 15.179 ms | 15.162 ms | 14.501 ms | 14.836 ms |
| 100 MB             | 203.85 ms | 210.14 ms | 220.38 ms | 216.34 ms | 211.12 ms |
| 1 GB               | 1.7674 s  | 1.8174 s  | 1.8396 s  | 1.8496 s  | 1.8774 s  |
