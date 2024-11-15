# Proving a file

To store the file according to the protocol, Storage Provider has to assign it to a sector, pre-commit and then prove it!
That's a lot of steps, but this is handled automatically, behind the scenes by the [pipeline](../architecture/polka-storage-provider-server.md#sealing-pipeline), then the file is eventually [published](../storage-provider-cli/client/index.md#publish-deal).

Here are excerpts from Storage Provider Node after executing the [store a file scenario](./demo-file-store.md):

```log
2024-11-11T12:34:21.430693Z  INFO start_rpc_server: polka_storage_provider_server::rpc: Starting RPC server at 127.0.0.1:8000
2024-11-11T12:34:21.430870Z  INFO start_upload_server: polka_storage_provider_server::storage: Starting HTTP storage server at: 127.0.0.1:8001
2024-11-11T12:34:21.431984Z  INFO start_rpc_server: polka_storage_provider_server::rpc: RPC server started
2024-11-11T12:35:07.883255Z  INFO request{method=PUT matched_path="/upload/:cid" request_id=e71d7e49-0272-435e-899e-a12a5d639268}:upload: polka_storage_provider_server::storage: CAR file created final_content_path="/var/folders/51/ch08ltd95bxcwpvskd28wr5h0000gp/T/Xvm5m7j/deals_storage/car/bafkreihoxd7eg2domoh2fxqae35t7ihbonyzcdzh5baevxzrzkaakevuvy.car"
2024-11-11T12:37:29.258216Z  INFO add_piece: polka_storage_provider_server::pipeline: Adding a piece...
2024-11-11T12:37:29.258785Z  INFO polka_storage_provider_server::pipeline: Preparing piece...
2024-11-11T12:37:29.259375Z  INFO polka_storage_provider_server::pipeline: Adding piece...
2024-11-11T12:37:29.261621Z  INFO add_piece: polka_storage_provider_server::pipeline: Finished adding a piece
2024-11-11T12:37:29.261979Z  INFO polka_storage_provider_server::pipeline: Add Piece for piece Commitment { commitment: [...], kind: Piece }, deal id 0, finished successfully.
2024-11-11T12:37:29.262023Z  INFO precommit: polka_storage_provider_server::pipeline: Starting pre-commit
2024-11-11T12:37:29.262258Z  INFO precommit: polka_storage_provider_server::pipeline: Padded sector, commencing pre-commit and getting last finalized block
2024-11-11T12:37:29.263185Z  INFO precommit: polka_storage_provider_server::pipeline: Current block: 35
2024-11-11T12:37:29.263852Z  INFO filecoin_proofs::api::seal: seal_pre_commit_phase1:start: SectorId(1)
2024-11-11T12:37:29.275251Z  INFO storage_proofs_porep::stacked::vanilla::proof: replicate_phase1
2024-11-11T12:37:29.275814Z  INFO storage_proofs_porep::stacked::vanilla::graph: using parent_cache[64 / 64]
2024-11-11T12:37:29.276105Z  INFO storage_proofs_porep::stacked::vanilla::cache: parent cache: opening /var/tmp/filecoin-parents/v28-sdr-parent-3f0eef38bb48af1f48ad65e14eb85b4ebfc167cec18cd81764f6d998836c9899.cache, verify enabled: false
2024-11-11T12:37:29.277633Z  INFO storage_proofs_porep::stacked::vanilla::proof: single core replication
2024-11-11T12:37:29.277644Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single: generate labels
2024-11-11T12:37:29.277681Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single: generating layer: 1
2024-11-11T12:37:29.277915Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single:   storing labels on disk
2024-11-11T12:37:29.278316Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single:   generated layer 1 store with id layer-1
2024-11-11T12:37:29.278328Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single:   setting exp parents
2024-11-11T12:37:29.278336Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single: generating layer: 2
2024-11-11T12:37:29.278418Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single:   storing labels on disk
2024-11-11T12:37:29.278735Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single:   generated layer 2 store with id layer-2
2024-11-11T12:37:29.278745Z  INFO storage_proofs_porep::stacked::vanilla::create_label::single:   setting exp parents
2024-11-11T12:37:29.278761Z  INFO filecoin_proofs::api::seal: seal_pre_commit_phase1:finish: SectorId(1)
[...]
2024-11-11T12:37:29.313831Z  INFO storage_proofs_core::data: dropping data /var/folders/51/ch08ltd95bxcwpvskd28wr5h0000gp/T/Xvm5m7j/deals_storage/sealed/1
2024-11-11T12:37:29.314137Z  INFO filecoin_proofs::api::seal: seal_pre_commit_phase2:finish
2024-11-11T12:37:29.314165Z  INFO precommit: polka_storage_provider_server::pipeline: Created sector's replica: PreCommitOutput { }
[...]
2024-11-11T12:37:57.324204Z  INFO precommit: polka_storage_provider_server::pipeline: Successfully pre-commited sectors on-chain: [SectorsPreCommitted { block: 39, [...] }]
2024-11-11T12:37:57.324292Z  INFO polka_storage_provider_server::pipeline: Precommit for sector 1 finished successfully.
2024-11-11T12:37:57.324345Z  INFO prove_commit: polka_storage_provider_server::pipeline: Starting prove commit
2024-11-11T12:37:57.325705Z  INFO prove_commit: polka_storage_provider_server::pipeline: Wait for block 49 to get randomness
2024-11-11T12:39:05.518784Z  INFO storage_proofs_porep::stacked::vanilla::proof: generating interactive vanilla proofs
2024-11-11T12:39:05.529259Z  INFO bellperson::groth16::prover::native: Bellperson 0.26.0 is being used!
2024-11-11T12:39:06.634632Z  INFO bellperson::groth16::prover::native: synthesis time: 1.105318708s
2024-11-11T12:39:06.634659Z  INFO bellperson::groth16::prover::native: starting proof timer
[...]
2024-11-11T12:39:23.728566Z  INFO bellperson::groth16::prover::native: prover time: 17.094277959s
2024-11-11T12:39:23.737186Z  INFO prove_commit: polka_storage_provider_server::pipeline: Proven sector: 1
```

After that, Storage Provider needs to continously [submit a PoSt](../architecture/pallets/storage-provider.md#submit_windowed_post) to prove that they are still storing the file. If they do not, they'll be slashed.
We have not yet integrated the logic for PoSt verification with Storage Provider node, but the logic on-chain has been implemented.

