# Polka Fetch

The polka-fetch is a CLI tool, used to retrieve data stored in the Polka Storage network. It connects to the appropriate storage provider using the libp2p protocol and retrieves the requested data.

## Usage

Polka Fetch can be used as follows:

```bash
polka-fetch [OPTIONS] <SUBCOMMAND> --output <OUTPUT_FILE>
```

### Global Options

| Option                | Description                                                                                                               |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `--output <PATH>`     | The output file path to write the downloaded data to (required)                                                           |
| `--overwrite`         | Whether to overwrite existing files if they already exist                                                                 |
| `--extract`           | Whether to extract the retrieved file after download                                                                      |
| `--timeout <SECONDS>` | Cancel the download if not completed after the specified duration in seconds. If not set, the download will never timeout |
| `--help`              | Print help information                                                                                                    |

### Subcommands

The tool provides two ways to download content:

#### By Payload CID

Download content directly using its payload CID and provider information:

```bash
polka-fetch --output <OUTPUT_FILE> by-payload-cid --provider <PROVIDER_MULTIADDR> --payload-cid <CID>
```

Options:

- `--provider <MULTIADDR>` - Provider multiaddress used for the data download (can be specified multiple times)
- `--payload-cid <CID>` - CID of the data being downloaded

Example:

```bash
polka-fetch --output ~/downloads/my-file.txt by-payload-cid --provider /ip4/127.0.0.1/tcp/5001 --payload-cid bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi
```

#### By Deal ID

Download content by referencing a storage deal ID. The tool will automatically:

1. Retrieve deal information from the blockchain
2. Find the storage provider responsible for the deal
3. Connect to the storage provider through the bootstrap node
4. Locate and download the content

```bash
polka-fetch --output <OUTPUT_FILE> by-deal-id --deal-id <DEAL_ID> --bootstrap-address <BOOTSTRAP_ADDR> --bootstrap-peer <PEER_ID> --parachain-address <PARACHAIN_URL>
```

Options:

- `--deal-id <DEAL_ID>` - The ID of the storage deal on the blockchain
- `--bootstrap-address <MULTIADDR>` - Bootstrap node address (multiaddress format)
- `--bootstrap-peer <PEER_ID>` - Bootstrap node peer ID
- `--parachain-address <URL>` - Parachain node WebSocket URL (e.g., "ws://127.0.0.1:9944")

Example:

```bash
polka-fetch --output ~/downloads/my-file.txt --extract by-deal-id --deal-id 42 --bootstrap-address /ip4/127.0.0.1/tcp/9988 --bootstrap-peer 12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp --parachain-address ws://127.0.0.1:9944
```
