# CAR server

It is an HTTP server that converts arbitrary content into a [CARv2](https://ipld.io/specs/transport/car/carv2/) file and serves it over HTTP - supporting the latest CARv2 format,
which is not yet entirely supported by other crates in the Rust ecosystem.
Following the next steps will allow to run the server locally and use it to upload and download files.

<div class="warning">
The server is a proof of concept, showcasing our CARv2 implementation, but it is not intended to be used in production.
Anyone can upload and download files without authentication or authorization.
</div>

## Start the server

1. Create a Docker volume to store uploaded files:

`docker volume create storage_provider`

2. Start the server:

```
docker run \
    -p 127.0.0.1:9000:9000 \
    --mount source=storage_provider,destination=/app/uploads \
    polkadotstorage.azurecr.io/polka-storage-provider:0.1.0 storage \
        --listen-addr 0.0.0.0:9000
```

- `-p 127.0.0.1:9000:9000`: Maps port `9000` on the localhost to port `9000` on the container.
- `--mount source=storage_provider,destination=/app/uploads`: Mounts the `storage_provider` volume to `/app/uploads` inside the container.
- `polkadotstorage.azurecr.io/polka-storage-provider:0.1.0 storage`: Runs the `polkadotstorage.azurecr.io/polka-storage-provider:0.1.0` image with the `storage` command.
- `--listen-addr 0.0.0.0:9000`: Configures the server to listen on all available network interfaces.

## Verifying the Setup

After setting up and starting the CAR server, it's essential to verify that everything works correctly.
Follow these steps to ensure your setup works as expected:

1. Upload a test file using the instructions in the [Upload a file](../storage-provider-cli/storage.md#upload-a-file) section. Make sure to note the CID returned by the server.

2. Download the CAR file using the retrieved CID, following the steps in the [Download the CAR File](../storage-provider-cli/storage.md#download-the-car-file) section.

3. [Optional] Verify the contents of the downloaded CAR file. For example: [go-car](https://github.com/ipld/go-car/tree/master/cmd/car#install)'s `inspect` command:

   ```bash
   car inspect <target_file>
   ```

   The user can use `debug` for more detailed output:

   ```bash
   car debug <target_file>
   ```

   If the user desires, they can extract the contents of the file:

   ```bash
   car extract <target_file>
   ```

If a file can be successfully uploaded, the server produces a CID, allows download the corresponding CAR file, and verify its contents, the CAR server setup is working correctly.
