# Peer Resolver

This library is intended for peer discovery in a libp2p network.
It abstracts libp2p [swarms][1], a bootstrap, register and discovery swarm.

## Protocols

The peer resolver library uses two protocol to resolve peer information.
The [identify protocol][10] is used by new peers during registration to exchange information such as public keys, known network addresses and peer IDs.
The [rendezvous protocol][2] is used by bootstrap and discovery swarms.
The bootstrap swarm uses this protocol to register new peers and serve discoveries to requesting peers.

## Bootstrap Swarm

The bootstrap swarm uses the [rendezvous server behaviour][3] to track peer registrations and share information with client nodes for discovery.
Bootstrap swarm should be well-known peers that share their [PeerID][4] and [multiaddress][5] so clients can connect to them and discover peers in the network.
Check the [bootstrap example][7] on how to use the peer resolver library to run a bootstrap swarm.

## Registration

The register swarm is used to register the peer with the rendezvous point.
The registration lifetime can be set using the optional `ttl` argument.
If the argument is not set it will default to the minimum lifetime of 2 hours.
The maximum lifetime of the registration is 72 hours.
These lifetime values are described in libp2p's [registration lifetime docs][6].
By calling the `register` function with the rendezvous point information it dials in to the address, sends the [identify protocol][10] message, registers its external address and registers with the rendezvous point.
Check the [register example][8] on how to use the peer resolver library to run a register swarm.

## Discovery

The discover swarm exposes several functions that aid in peer discovery.
The `dial` function allows peers to dial into the rendezvous point address and establish a connection.
Once the connection is established, the swarm uses the `discover` function to request peer information from the bootstrap node.
The bootstrap node respond with the `Discovered` event, this event contains all the peers that are registered in the network and a new rendezvous cookie. For continuous discovery, the discovery swarm should call `replace_cookie` function with the cookie captured in the `Discovered` event.
In the [discovery example][9], you can see how a discovery swarm can be used to build and update a database with known and active peers.

## Examples

To run the examples, follow these steps:

1. Start the rendezvous point by running the following command:

```bash
RUST_LOG=info cargo run --example bootstrap
```

This command starts the rendezvous server, which will listen for incoming connections and handle peer registrations and discovery.

2. Register a peer by running the following command:

```bash
RUST_LOG=info cargo run --example identify
```

This command registers a peer with the rendezvous server, allowing the peer to be discovered by other peers.

3. Try to discover the registered peer from the previous step by running the following command:

```bash
RUST_LOG=info cargo run --example discovery
```

This command attempts to continuously discover the registered peer using the rendezvous server at an interval of 2 seconds.
Any newly registered peers will be logged.

[1]: https://docs.rs/libp2p/latest/libp2p/struct.Swarm.html
[2]: https://github.com/libp2p/specs/blob/master/rendezvous/README.md
[3]: https://docs.rs/libp2p/latest/libp2p/rendezvous/server/struct.Behaviour.html
[4]: https://docs.libp2p.io/concepts/fundamentals/peers/#peer-id
[5]: https://github.com/libp2p/specs/blob/master/addressing/README.md#multiaddr-in-libp2p
[6]: https://github.com/libp2p/specs/blob/d21418638d5f09f2a4e5a1ceca17058df134a300/rendezvous/README.md#registration-lifetime
[7]: ./examples/bootstrap.rs
[8]: ./examples/register.rs
[9]: ./examples/discovery.rs
[10]: https://github.com/libp2p/specs/blob/master/identify/README.md
