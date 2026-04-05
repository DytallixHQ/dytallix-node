# Dytallix Node

Source for the live Dytallix testnet node and its local path dependencies.

This repository was packaged from the running testnet server on April 5, 2026 so the public chain backend is no longer stranded on a single machine. It includes the server-side fix that makes the public signed transaction API usable end to end:

- signed ML-DSA-65 transactions are accepted by the live node
- the signed `fee` field is converted into execution gas at submit time
- `/status` exposes public gas parameters for SDK and CLI fee estimation

## Layout

- `dytallix-fast-launch/node`: public RPC node and execution engine
- `blockchain-core`: shared chain logic used by the node
- `pqc-crypto`: post-quantum crypto primitives and CLIs
- `smart-contracts`: Dytallix contract runtime and examples

## Build

Build the public node from its crate directory:

```bash
cd dytallix-fast-launch/node
cargo build --release --locked
```

## Notes

- This is a cleaned source snapshot. Runtime data, RocksDB files, launch evidence, local keys, Finder metadata, and temporary backup artifacts were intentionally excluded.
- `pqc-crypto` referenced a missing local dev dependency on the server (`../interoperability`). That dev-only dependency was removed here so the published tree builds cleanly.
