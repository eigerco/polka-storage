# CAR server

It is a HTTP server that enables us to convert arbitrary content into a [CARv2](https://ipld.io/specs/transport/car/carv2/) file and serve it over HTTP. Supporting the latest CARv2 format, which is not yet supported by other creates in the ecosystem. By following the next steps, you will be able to run the server locally and use it to upload and download files.

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

## Upload a file

The server exposes `POST /upload` which accepts arbitrary bytes.

Example usage with `curl`:

```
curl \
    -X POST \
    --data-binary "@image.jpg" \
    http://localhost:9000/upload
```

This command uploads the file `image.jpg` to the server running at `http://localhost:9000/upload`. The server converts the uploaded content to a CAR file and saves it to the mounted volume. The returned [Cid](https://github.com/multiformats/cid) can later be used to fetch a CAR file from the server.

## Download the CAR File

After uploading, you will receive a CID (Content Identifier) for the file. Use this CID to download the corresponding CAR file. Replace `:cid` with the actual CID provided:

```
curl \
    -X GET \
    --output ./content.car \
    http://localhost:9000/download/:cid
```

- `-X GET`: Specifies the GET request method.
- `http://localhost:9000/download/:cid`: The URL to download the CAR file, with :cid being the placeholder for the actual CID.
- `--output ./content.car`: Saves the downloaded CAR file as content.car in the current directory.
