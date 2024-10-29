# Mater CLI

 <!-- NOTE(@jmg-duarte,24/10/2024): ideally we'd point to the docs.rs of mater too, hopefully we can get mater and the cli published asides from this -->

The Mater CLI is used by storage clients to convert files to CARv2 format and extract CARv2 content.

## Convert

The convert command converts a file to CARv2 format.

`mater-cli convert <INPUT_PATH> [OUTPUT_PATH]`

| Argument        | Description                                                                                                        |
| --------------- | ------------------------------------------------------------------------------------------------------------------ |
| <INPUT_PATH>    | Path to input file                                                                                                 |
| \[OUTPUT_PATH\] | Optional path to output CARv2 file. If no output path is given it will store the `.car` file in the same location. |

## Extract

Convert a CARv2 file to its original format.

`mater-cli extract <INPUT_PATH> [OUTPUT_PATH]`

| Argument        | Description                                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| <INPUT_PATH>    | Path to CARv2 file                                                                                                             |
| \[OUTPUT_PATH\] | Optional path to output file. If no output path is given it will remove the extension and store the file in the same location. |
