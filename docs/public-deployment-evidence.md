# Public Deployment Evidence

[Docs hub](README.md) | [Build and run](build-and-run.md) | [Deployment separation audit](deployment-separation-audit.md)

## What Publicly Verifiable Evidence Exists Today

The live public node currently exposes:

- `GET https://dytallix.com/status`
- `GET https://dytallix.com/api/capabilities`
- `GET https://dytallix.com/api/blockchain/api/staking/validators`

Those endpoints are enough to verify that the live public behavior matches the
published capability contract in this repo on April 13, 2026:

- the live node advertises `/api/capabilities`
- public staking and governance writes are marked hidden in the capability contract
- validator discovery no longer returns placeholder IDs like `validator1`

## What This Does Not Prove

This public evidence does not, by itself, prove that the production host has
already been rebuilt and cut over from a clean checkout rooted at
`/opt/dytallix-node`.

That final provenance check still requires operator-side evidence from the real
host, such as:

- active process path under `/opt/dytallix-node`
- clean checkout revision hash
- build command and resulting binary path
- restart evidence from `systemd` or `pm2`

Until that evidence is published, this repository should still describe itself
as a published snapshot plus a reproducible deployment path.

## Operator Audit Snapshot

Host audit on April 13, 2026 confirmed the following:

- PM2 process `dytallix-node` is online
- active binary path at the time of audit: `/opt/dytallix-node/dytallix-fast-launch/node/target/release/dytallix-fast-node`
- process working directory: `/opt/dytallix-node`
- deployed commit: `00b1ebbffc2c2f261de06c03c662957ca52cf2f9`
- dirty files in the live checkout:
	- `dytallix-fast-launch/node/src/main.rs`
	- `dytallix-fast-launch/node/src/rpc/mod.rs`
	- untracked `docs/public-capabilities.json`
- clean comparison checkout: `/opt/dytallix-node-clean` at the same commit, with clean git status

For reproducible clean-checkout builds from this repository, the release binary is
emitted at `/opt/dytallix-node/target/release/dytallix-fast-node` when built
from the workspace root with the documented `cargo build -p dytallix-fast-node
--bin dytallix-fast-node --release --locked` command.

That means the live node behavior and the published node contract can be
verified today, but production provenance is still not proven from a clean
public deployment commit. The cutover from hidden host-side source drift to a
clean canonical checkout is still pending.

## Public Verification Command

Run this from the repo root:

```bash
python3 scripts/check_live_public_deployment.py
```