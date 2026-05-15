# Tranche 3 Evidence Packet

Generated: 2026-05-04T20:13:03Z
Updated: 2026-05-15T20:16:41Z

## Simple Takeaway

T3 is complete for bridge, liquidation, and dashboard evidence.

This packet backs the demo video with the exact evidence reviewers need: the Sepolia to Stellar bridge path, the Soroban liquidation path, the dashboard surface, and the regression tests.

The one thing we are not pretending is live is wallet-specific position data for the recording wallet. That means the connected Freighter wallet used for the demo does not own its own seeded vault yet, so the dashboard labels the position as `wallet fallback` instead of calling it `live`.

## What We Completed

- Bridge evidence: Sepolia source transaction, Stellar destination execution, deployed bridge handler, hstETH token, and vault addresses.
- Liquidation evidence: Soroban vault liquidation path plus the Rust liquidation bot.
- Dashboard evidence: live Soroban RPC health, Freighter states, bridge proof, risk panel, Blend readiness, contract surface, and T3 Gate.
- Test evidence: dashboard model/live-data tests, browser regression, workspace contract tests, and liquidation-bot test coverage.

## Bridge Evidence

The bridge story is not a mock. The packet ties together deployed contracts, the source transaction, the Stellar execution, and the dashboard read-back.

- Sepolia source transaction: `0xeb1863825a0d73c0cc67eae3e3fb5edb574ab615ebf002c0645c3eefbd7a3fb9`
- Stellar execute transaction: `0e55c7d53ab84a1953b68eef0913b05f00870d05285b4e00847342f9a8f3dce6`
- Stellar ledger: `2379754`
- Executed at: `2026-05-04T14:49:46Z`
- Axelar Migrator: `0x5A33F35f4B02269107e60713bc2dAb970C741a0c`
- Bridge Handler: `CCBI7ZKMKOEHUCOLBXW63QKMFN5MFIDANODW5L4IO4RC5XPCD2IEDTQY`
- Bridge hstETH Token: `CC366YM6MJOISQSCUXBU3BCRNKVCDI7VOZT3SF7AJKL7ILTMXY3AGBJ2`
- Collateral Vault: `CAGG2XJJJGTER3E5BP26FVI3QLT4QYKZT233ZSPUC5O573QRU3D2Y7TW`

The dashboard also shows this in product form under Bridge Proof: source hash, Stellar execute hash, and executed status.

## Liquidation Evidence

The liquidation path is implemented, tested, and visible in the dashboard risk view.

- Vault contract has a liquidation path with liquidator role checks.
- Oracle adaptor supports TWAP, staleness checks, deviation handling, and safe mode.
- Liquidation bot scans positions, simulates liquidation first, checks liquidator funds, and only submits after simulation succeeds.
- Dashboard risk view shows buffer to liquidation, oracle freshness, and safe mode.

## Dashboard Evidence

The recording surface is the Stellar Terminal at:

`http://localhost:4173/apps/stellar-dashboard/`

Live browser check after Freighter unlock:

- Wallet CTA: `GCI66ISW...3DDABB`
- Wallet metric: `connected`
- RPC metric: `healthy`
- Data Mode: `wallet fallback`
- Position Source: `wallet fallback`
- Position Source detail: `live wallet read unavailable`
- T3 Gate, Freighter Dashboard: `connected`
- T3 Gate, Position Adapter: `fallback`

Read-only production-like route check:

- Route: `http://localhost:4173/apps/stellar-dashboard/?fresh=p4-production-origin#activity`
- Dashboard loads from `/apps/stellar-dashboard/`
- RPC metric: `healthy`
- Wallet metric: `available`
- Wallet detail: `not connected`
- Position Source: `static evidence`
- T3 Gate, Position Adapter: `complete`
- T3 Gate, Freighter Dashboard: `needs live verification`
- T3 Gate, T3 Evidence Packet: `complete`

Final demo-video visual pass:

- Date: `2026-05-15T20:16:41Z`
- Dashboard route verified locally.
- Screenshot evidence captured at `/private/tmp/stellar-dashboard-demo-final.png`.
- Freighter unavailable copy now reads `Install Freighter` / `not installed`.
- No signing, wallet seeding, or transaction mutation was performed during the visual pass.

## Test Evidence

Final dashboard verification, 2026-05-15T20:16:41Z:

- `make stellar-dashboard-test`: passed.
- Node suites: 4/4.
- Node tests: 26/26.
- Browser regression: 3/3 passed.
- Browser cases:
  - `freighter unavailable load`
  - `connected wallet fallback and navigation`
  - `connected wallet no position state`
- `node --check apps/stellar-dashboard/src/main.mjs`: passed.
- `node --check apps/stellar-dashboard/src/liveData.mjs`: passed.
- `node --check scripts/regression-stellar-dashboard.mjs`: passed.

Contract and bot verification, 2026-05-12T21:12:22Z:

- `cargo test --all`: passed; 73 workspace contract tests.
- `cargo test --manifest-path liquidation-bot/Cargo.toml`: passed; 1 liquidation-bot test.
- `git diff --check`: passed.

Workspace contract coverage observed:

- `bridge-handler`: 15 tests passed.
- `bridge-poc`: 2 tests passed.
- `blend-oracle-adaptor`: 10 tests passed.
- `mock-bridge`: 1 test passed.
- `mock-oracle`: 3 tests passed.
- `oracle-adaptor`: 9 tests passed.
- `token`: 18 tests passed.
- `vault`: 15 tests passed.

## What We Are Not Claiming

- We are not claiming wallet-specific `live` position data for the demo Freighter wallet.
- We are not claiming that this recording wallet owns a seeded vault.
- We are not claiming that the demo video performs signing or submits a new transaction.

The dashboard is intentionally honest here: if a connected wallet has no seeded test vault, the app says `wallet fallback`. A separate seeded-wallet pass can be recorded later if reviewers require wallet-specific `live` position data.
