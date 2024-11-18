### Prerequisites

Script expects `mater-cli`, `polka-storage-provider-client`, `polka-storage-provider-server`, `storagext-cli` in the current directory.

1. Start zombienet and call required extrinsics
   - Setup Alices balances `./storagext-cli --sr25519-key //Alice market add-balance 250000000000`
   - Setup Charlies balances `./storagext-cli --sr25519-key //Charlie market add-balance 250000000000`
   - Register Charlie as a storage provider `./storagext-cli --sr25519-key "//Charlie" storage-provider register "peer_id"`
2. Generate proving params
   - `./polka-storage-provider-client proofs porep-params`
3. Start storage provider server
   - `./polka-storage-provider-server --sr25519-key //Charlie --seal-proof "2KiB" --post-proof "2KiB" --porep-parameters 2KiB.porep.params`
