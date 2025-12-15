# Runtime Upgrade Guide

This document explains the exact steps for preparing, testing, and executing a runtime upgrade on the Polka Storage parachain.
The goal is a repeatable workflow that any engineer on the team can follow safely.

## Overview

A runtime upgrade follows three phases:

1. Prepare the new runtime
2. Test the upgrade using a Chopsticks fork of Paseo
3. Execute the upgrade on the live Paseo chain

This guide covers all three phases.

## Prepare the new runtime

### Implement on_runtime_upgrade migration

If the new runtime changes any storage types, you must include a migration to avoid decode failures.
For the storage provider pallet, the simplest migration is to clear incompatible entries.

Example:

```rust
fn on_runtime_upgrade() -> Weight {
    let _ =
        StorageProviders::<T>::clear(StorageProviders::<T>::iter().count() as u32, None);
    T::DbWeight::get().reads(1)
}
```

The `clear` call removes storage keys without decoding values. This avoids runtime panics when types change. You can also clear additional storage items as needed.

### Bump the runtime version

In the runtime definition update:

- `spec_version` (increment by 1).
- `impl_version` (increment as needed).

Never reuse previous version numbers.

### Build the compressed WASM runtime

Build the runtime in release mode:

`just build-runtime`

or

`cargo b --release -F testnet -p polka-storage-runtime`

This generates:

- `polka_storage_runtime.compact.wasm`
- `polka_storage_runtime.compact.compressed.wasm` (use this one for upgrades)

Verify the artifact:

`subwasm info target/release/wbuild/polka-storage-runtime/polka_storage_runtime.compact.compressed.wasm`

Confirm that:

- Metadata is valid
- Version numbers match your changes
- WASM size is acceptable

## Rehearse the upgrade using a Chopsticks fork

Always test the upgrade on a fork before touching the live chain.

### Start a Chopsticks fork of the testnet

Start Chopsticks with your config:

```bash
npx @acala-network/chopsticks --config=./polka-storage.yml
```

Where `polka-storage.yml` looks like:

```yaml
endpoint:
  - wss://collator.polka-storage.eiger.co
db: ./polka-storage-fork.sqlite
mock-signature-host: true
runtime-log-level: 5

import-storage:
  System:
    Account:
      - - - 5DegWdr4EoDZLPEM4dzXq1TJpTMc2eaMop9nhEFHVDoYYYLK
        - providers: 1
          data:
            free: "2000000000000000000000"
```

### Import the Sudo account in polkadot js apps

Chopsticks does not load local keys by default.
Import your testnet sudo account so that Developer -> Sudo appears in the UI.

### Submit the runtime upgrade

Navigate to:

`Developer -> Sudo`

Choose:

`system -> setCode`

Upload the compressed WASM artifact (`polka_storage_runtime.compact.compressed.wasm`) and submit the transaction with the sudo account.

### Advance the block

In another terminal:

```bash
curl -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"dev_newBlock","params":[{"count":1}]}' \
  http://127.0.0.1:8000
```

This triggers the runtime upgrade and executes on_runtime_upgrade.

### Validate the state after upgrade

Confirm the following:

- Runtime version updated correctly.
- StorageProviders is cleared or properly migrated.
- No storage decode errors.
- Frontend can query provider data without Unknown errors.
- Providers can register again.
- Deals can be published and removed.

If the Chopsticks rehearsal fails, do not upgrade Paseo.
