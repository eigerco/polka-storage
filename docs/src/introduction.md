# Introduction

Welcome to the Polka Storage project!

This project's goal is to deliver a Polkadot-native system parachain for data storage.

Since the Referendum approval we've been busy developing the parachain,
this is our deliverable for Phase 1, composed of:

- Filecoin actor ports:
  - [Storage Provider](./pallets/storage-provider.md) — excluding proving mechanisms
  - [Market](./pallets/market.md)
- [CAR file conversion server](./storage-provider-cli/storage.md)
- Dedicated CLIs
  - [`storage-provider-cli`](./storage-provider-cli/storage.md) to generate keys and test our CARv2 Rust implementation!
  - [`storagext-cli`](./storagext-cli/index.md) (shown below) to take the parachain for a spin!
<p>
    <img
        src="images/showcase/cli_basic.gif"
        alt="Polka Storage CLI tooling showcase">
</p>

You can read more about the project in:

- Treasury Proposal — <https://polkadot.polkassembly.io/post/2107>
- OpenGov Referendum — <https://polkadot.polkassembly.io/referenda/494>
- Research Report — <https://github.com/eigerco/polkadot-native-storage/blob/main/doc/report/polkadot-native-storage-v1.0.0.pdf>
- Polkadot Forum News Post — <https://forum.polkadot.network/t/polkadot-native-storage/4551>

---

<p>
    <a href="https://eiger.co">
        <img
            src="images/logo.svg"
            alt="Eiger Oy"
            style="height: 50px; display: block; margin-left: auto; margin-right: auto; width: 50%;">
    </a>
</p>
