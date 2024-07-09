## Build the Docker Image

To build the Docker image for the provider, execute the following command in the project directory.

`docker build -t eiger/polka-storage-provider --file ./docker/dockerfiles/storage-provider/Dockerfile .`

This command uses the custom Dockerfile located at `./docker/dockerfiles/storage-provider/Dockerfile` to create an image named `eiger/polka-storage-provider`.

## Start the Storage Provider Server

### Create a Docker Volume

First, create a Docker volume that the storage provider will use to store uploaded files:

`docker volume create storage_provider`

This command creates a volume named storage_provider.

### Start the Storage Server

Next, start the storage server using the created volume:

```
docker run \
    -p 127.0.0.1:9000:9000 \
    --mount source=storage_provider,destination=/app/uploads \
    eiger/polka-storage-provider storage \
        --listen-addr 0.0.0.0:9000
```

- `-p 127.0.0.1:9000:9000`: This maps port `9000` on the localhost to port `9000` on the container.
- `--mount source=storage_provider,destination=/app/uploads`: Mounts the `storage_provider` volume to `/app/uploads` inside the container.
- `eiger/polka-storage-provider storage`: Runs the `eiger/polka-storage-provider` image with the `storage` command.
- `--listen-addr 0.0.0.0:9000`: Configures the server to listen on all available network interfaces.

## Upload a file

To upload a file to the provider's server, use the following curl command. Replace image.jpg with the path to your file:

```
curl \
    -X POST \
    --data-binary "@image.jpg" \
    http://localhost:9000/upload
```

This command uploads the file `image.jpg` to the server running at `http://localhost:9000/upload`. The server converts the uploaded content to a CAR file and saves it to the mounted volume. The returned Cid can later be used to fetch a CAR file from the server.

## Download the CAR File

After uploading, you will receive a CID (Content Identifier) for the file. Use this CID to download the corresponding CAR file. Replace `:cid` with the actual CID provided:

```
curl -v \
    -X GET \
    --output ./content.car \
    http://localhost:9000/download/:cid
```

- `-v`: Enables verbose mode, providing detailed information about the request.
- `-X GET`: Specifies the GET request method.
- `http://localhost:9000/download/:cid`: The URL to download the CAR file, with :cid being the placeholder for the actual CID.
- `--output ./content.car`: Saves the downloaded CAR file as content.car in the current directory.

By following these steps, you can successfully build the Docker image, start the storage provider server, upload a file, and download the CAR file.
