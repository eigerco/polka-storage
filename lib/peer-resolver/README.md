# Peer Resolver

This library is intended for peer discovery in a libp2p network.
It abstracts libp2p [swarms][1], a bootstrap and client swarm, using the [rendezvous protocol][2] for peer discovery.

## Bootstrap Swarm

The bootstrap swarm uses the [rendezvous server behaviour][3] to track peer registrations and share information with client nodes for discovery. Bootstrap swarm should be well-known peers that share their [PeerID][4] and [multiaddress][5] so clients can connect to them and discover peers in the network. Check the [bootstrap example][7] on how to use the peer resolver library to run a bootstrap swarm.

## Registration

The client swarm exposes a `register` function which allows the peer to register to a given namespace with the bootstrap node. The rendezvous point (bootstrap node [PeerID][4]) and the rendezvous point address (bootstrap node [multiaddress][5]) are given as arguments so the peer can add the bootstrap node as an external address and dial in to it to register itself. The duration of the registration can be given as an optional argument in seconds. The minimum duration is 2 hours and the maximum duration is 72 hours as recommended in the rendezvous spec for [registration lifetime][6]. If no duration is passed in the registration duration will be set to 2 hours. Check the [register example][8] on how to use the peer resolver library to register as a new peer in the network.

## Discovery

The client swarm exposes several functions that aid in peer discovery. The `dial` function allows peers to dial into the rendezvous point address and establish a connection. Once the connection is established, the client uses the `discover` function to request peer information from the bootstrap node. The bootstrap node respond with the `Discovered` event, this event contains all the peers that are registered in the network and a new rendezvous cookie. For continuous discovery, the client swarm should call `replace_cookie` function with the cookie captured in the `Discovered` event. In the [discovery example][9], you can see how a client swarm can be used to build and update a database with known and active peers.

[1]: https://docs.rs/libp2p/latest/libp2p/struct.Swarm.html
[2]: https://github.com/libp2p/specs/blob/master/rendezvous/README.md
[3]: https://docs.rs/libp2p/latest/libp2p/rendezvous/server/struct.Behaviour.html
[4]: https://docs.libp2p.io/concepts/fundamentals/peers/#peer-id
[5]: https://github.com/libp2p/specs/blob/master/addressing/README.md#multiaddr-in-libp2p
[6]: https://github.com/libp2p/specs/blob/d21418638d5f09f2a4e5a1ceca17058df134a300/rendezvous/README.md#registration-lifetime
[7]: ./examples/bootstrap.rs
[8]: ./examples/register.rs
[9]: ./examples/discovery.rs
