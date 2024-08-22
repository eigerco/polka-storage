# CAR server

It is a HTTP server that enables us to convert arbitrary content into a [CARv2](https://ipld.io/specs/transport/car/carv2/) file and serve it over HTTP. Supporting the latest CARv2 format, which is not yet entirely supported by other crates in the Rust ecosystem. By following the next steps, you will be able to run the server locally and use it to upload and download files.

<div class="warning">
The server is a proof of concept, showcasing CARv2 implementation, and is not intended to be used in production. Anyone can upload and download files without authentication or authorization.
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

After setting up and starting the CAR server, it's important to verify that everything is working correctly. Follow these steps to ensure your setup works as expected:

1. Upload a test file using the instructions in the [Upload a file](../storage-provider-cli/storage.md#upload-a-file) section. Make sure to note the CID returned by the server.

2. Download the CAR file using the CID you received, following the steps in the [Download the CAR File](../storage-provider-cli/storage.md#download-the-car-file) section.

3. Verify the contents of the downloaded CAR file. You can use [go-car](https://github.com/ipld/go-car/tree/master/cmd/car)'s `extract` command.

If you can successfully upload a file, receive a CID, download the corresponding CAR file, and verify its contents, your CAR server setup is working correctly.
