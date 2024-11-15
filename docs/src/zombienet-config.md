# Zombienet Configuration Breakdown

Running the Zombienet requires a configuration file. This configuration file is downloaded during the third step of [Linux](#linux-x86_64)/[MacOS](#macos-arm) or can be copied from the first step of [Running the parachain](#running-the-parachain).

## Similarities

The two files share most of the contents, so we'll start by covering their similarities.
For more details refer to the [`zombienet` documentation](https://paritytech.github.io/zombienet/network-definition-spec.html):

### `relaychain`

| Name              | Description                                   |
| ----------------- | --------------------------------------------- |
| `chain`           | The relaychain name                           |
| `default_args`    | The default arguments passed to the `command` |
| `default_command` | The default command to run the relaychain     |
| `nodes`           | List of tables defining the nodes to run      |

#### `nodes`

| Name        | Description                            |
| ----------- | -------------------------------------- |
| `name`      | The node name                          |
| `validator` | Whether the node is a validator or not |

### `parachains`

A list of tables defining multiple parachains, in our case, we only care for our own parachain.

| Name            | Description                                                   |
| --------------- | ------------------------------------------------------------- |
| `cumulus_based` | Whether to use `cumulus` based generation                     |
| `id`            | The parachain ID, we're using `1000` as a placeholder for now |
| `collators`     | List of tables defining the collators                         |

#### `collators`

| Name        | Description                              |
| ----------- | ---------------------------------------- |
| `args`      | The arguments passed to the `command`    |
| `command`   | The command to run the collator          |
| `name`      | The collator name                        |
| `validator` | Whether the collator is also a validator |

## Differences

The difference between them lies in the usage of container configurations:

| Name                 | Description                                                                                                                                                                   |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `image_pull_policy`  | Defines when `zombienet` should pull an image; read more about it in the [Kubernetes documentation](https://kubernetes.io/docs/concepts/containers/images/#image-pull-policy) |
| `image`              | Defines which image to pull                                                                                                                                                   |
| `ws_port`/`rpc_port` | Depending on the type of configuration (Native or Kubernetes), this variable sets the port for the collator RPC service                                                       |
