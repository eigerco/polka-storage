# Mater CLI

 <!-- NOTE(@jmg-duarte,24/10/2024): ideally we'd point to the docs.rs of mater too, hopefully we can get mater and the cli published asides from this -->

The Mater CLI is used by storage clients to convert files to the CARv2 format and extract CARv2 content.

> Currently only the CARv2 format is supported in the binary since the network requires the CARv2 format.
> However, *the `mater` library has full support for CARv1*.

To learn more about the CAR format, please refer to the official specifications:
* CARv1 — <https://ipld.io/specs/transport/car/carv1/>
* CARv2 — <https://ipld.io/specs/transport/car/carv2/>

## `convert`

The convert command converts a file to CARv2 format.

`mater-cli convert <INPUT_PATH> [OUTPUT_PATH]`

| Argument        | Description                                                                                                        |
| --------------- | ------------------------------------------------------------------------------------------------------------------ |
| `<INPUT_PATH>`  | Path to input file                                                                                                 |
| `[OUTPUT_PATH]` | Optional path to output CARv2 file. If no output path is given it will store the `.car` file in the same location as the input file. |
| `-q`/`--quiet` | If enabled, only the resulting CID will be printed. |
| `--overwrite` | If enabled, the output will overwrite any existing files. |

### Example

```bash
$ mater-cli convert random1024.piece
Converted examples/random1024.piece and saved the CARv2 file at examples/random1024.car with a CID of bafkreidvyofebclo4kny43vpoe5kejg3mqtpq2eemaojzyvlwikwdvusxy
```

You can verify the output file using [`go-car`](https://github.com/ipld/go-car):

```bash
$ car inspect examples/random1024.car
Version: 2
Characteristics: 00000000000000000000000000000000
Data offset: 51
Data (payload) length: 1121
Index offset: 1172
Index type: car-multihash-index-sorted
Roots: bafkreidvyofebclo4kny43vpoe5kejg3mqtpq2eemaojzyvlwikwdvusxy
Root blocks present in data: Yes
Block count: 1
Min / average / max block length (bytes): 1024 / 1024 / 1024
Min / average / max CID length (bytes): 36 / 36 / 36
Block count per codec:
        raw: 1
CID count per multihash:
        sha2-256: 1
```


## `extract`

Convert a CARv2 file to its original format.

`mater-cli extract <INPUT_PATH> [OUTPUT_PATH]`

| Argument        | Description                                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `<INPUT_PATH>`  | Path to CARv2 file                                                                                                             |
| `[OUTPUT_PATH]` | Optional path to output file. If no output path is given it will remove the extension and store the file in the same location. |

### Example

```bash
$ mater-cli extract examples/random1024.car
Successfully converted CARv2 file examples/random1024.car and saved it to to examples/random1024
```

Conversly, you can also extract files generated using `car`:
```bash
# --no-wrap is necessary since mater does not perform wrapping
$ car create --no-wrap -f examples/random1024.go.car examples/random1024.piece
```

```bash
$ cargo run -r --bin mater-cli extract examples/random1024.go.car
Successfully converted CARv2 file examples/random1024.go.car and saved it to to examples/random1024.go
```
