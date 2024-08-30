# The `storage` command

The `storage` command launches a server that can convert files into CAR files.
It has two parameters that you can customize:

| Parameter              | Description                                             | Default value    |
| ---------------------- | ------------------------------------------------------- | ---------------- |
| `--listen-addr <ADDR>` | The address (including the port) for the storage server | `127.0.0.1:9000` |
| `--storage-dir <DIR>`  | The directory where the files will be stored            | `$PWD/uploads`   |

### Start the Storage Server

Next, start the storage server using the created volume:

```bash
polka-storage-provider storage --listen-addr 0.0.0.0:9000
```

- `--listen-addr 0.0.0.0:9000`: Configures the server to listen on all available network interfaces.

## Upload a file

To upload a file to the provider's server, use the following curl command. Replace `image.jpg` with the path to your file:

```bash
curl \
    -X POST \
    --data-binary "@image.jpg" \
    http://localhost:9000/upload
```

This command uploads the file `image.jpg` to the server running at `http://localhost:9000/upload`. The server converts the uploaded content to a CAR file and saves it to the mounted volume. The returned CID can later be used to fetch a CAR file from the server.

## Download the CAR File

After uploading, you will receive a CID (Content Identifier) for the file. Use this CID to download the corresponding CAR file. Replace `:cid` with the actual CID provided:

```bash
curl \
    -X GET \
    --output ./content.car \
    http://localhost:9000/download/<cid>
```

- `-X GET`: Specifies the GET request method.
- `http://localhost:9000/download/:cid`: The URL to download the CAR file, with `<cid>` being the placeholder for the actual CID.
- `--output ./content.car`: Saves the downloaded CAR file as content.car in the current directory.

Here's a quick example:

```bash
curl \
    -X GET \
    --output ./content.car \
    http://localhost:9000/download/bafkreicqsawmxavfxpqncjy545bgulr54b5xliriexxjiaof6uue5ovduu
```
