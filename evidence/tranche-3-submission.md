# Tranche 3 Evidence Packet

Updated: 2026-05-12

## Readiness Call

T3 is ready for internal submission review with the current evidence set. External submission is conditional on accepting the Freighter wallet position fallback as an intentional state: the dashboard connects to Freighter and classifies the wallet-specific position adapter as `fallback` when a seeded vault position is unavailable. If the T3 bar requires wallet-specific `live` position data for the connected Freighter account, the remaining blocker is seeding or discovering a live wallet vault position, which requires explicit approval for any signing or transaction prompt.

## Completed Scope

- Freighter connection UX distinguishes `connected`, `unavailable`, `wallet fallback`, `no wallet position`, and `static evidence` states.
- Dashboard Data Mode and Position Source are no longer ambiguous when connected-wallet reads fall back to seeded evidence.
- Hash navigation for Activity and Position sections was fixed and verified after live browser testing.
- Automated browser regression coverage was added for unavailable Freighter, connected wallet fallback, connected wallet empty-position, reload persistence, and hash navigation.
- Contract and adapter test coverage remains green across workspace contracts, liquidation-bot, dashboard model, dashboard live data, and Stellar position adapter modules.

## Browser And Freighter Evidence

Live browser check after Freighter unlock:

- Wallet CTA: `GCI66ISW...3DDABB`
- Wallet metric: `connected`
- RPC metric: `healthy`
- Data Mode: `wallet fallback`
- Position Source: `wallet fallback`
- Position Source detail: `live wallet read unavailable`
- T3 Gate, Freighter Dashboard: `connected`
- T3 Gate, Position Adapter: `fallback`
- Navigation: `#activity` lands on Activity/T3 Gate; `#position` returns to Vault Position after the scroll fix.

Automated browser regression:

- Command: `make stellar-dashboard-test`
- Browser result: `stellar dashboard browser regression: 3/3 passed`
- Cases:
  - `freighter unavailable load`
  - `connected wallet fallback and navigation`
  - `connected wallet no position state`

P4 production-like origin check:

- Route: `http://localhost:4173/apps/stellar-dashboard/?fresh=p4-production-origin#activity`
- Browser: real Chrome profile
- Origin behavior: dashboard loads from the static `/apps/stellar-dashboard/` path, Activity hash target is visible, and the T3 Gate renders below Activity.
- RPC metric: `healthy`
- Wallet metric: `available`
- Wallet detail: `not connected`
- Position Source: `static evidence`
- Position Source detail: `seeded testnet evidence`
- T3 Gate, Position Adapter: `complete`
- T3 Gate, Freighter Dashboard: `needs live verification`
- T3 Gate, T3 Evidence Packet: `complete`
- UX correction verified: passive Freighter availability does not display the seeded static address as the wallet address before connection.
- Manual connect was not attempted because approving a Freighter origin-permission prompt must remain a user-approved browser action.

## Test Evidence

Final pre-submit verification, 2026-05-12T21:12:22Z:

- `make stellar-dashboard-test`: passed after localhost bind was allowed; 4 Node suites / 26 tests passed and browser regression 3/3 passed.
- `cargo test --all`: passed; 73 workspace contract tests passed.
- `cargo test --manifest-path liquidation-bot/Cargo.toml`: passed; 1 test passed.
- `git diff --check`: passed.

Note: the first sandboxed dashboard regression attempt failed with `listen EPERM: operation not permitted 127.0.0.1`; rerun with localhost bind permission passed.

Focused dashboard and adapter suite:

- Command: `make stellar-dashboard-test`
- Result: passed
- Node test result: 4 suites, 26 tests passed
- Browser regression result: 3 cases passed

Workspace contract suite:

- Command: `cargo test --all`
- Result: passed
- Coverage observed:
  - `bridge-handler`: 15 tests passed
  - `bridge-poc`: 2 tests passed
  - `blend-oracle-adaptor`: 10 tests passed
  - `mock-bridge`: 1 test passed
  - `mock-oracle`: 3 tests passed
  - `oracle-adaptor`: 9 tests passed
  - `token`: 18 tests passed
  - `vault`: 15 tests passed

Liquidation bot:

- Command: `cargo test --manifest-path liquidation-bot/Cargo.toml`
- Result: passed
- Coverage observed: 1 test passed

Syntax and whitespace checks:

- Command: `node --check scripts/regression-stellar-dashboard.mjs`
- Result: passed
- Command: `node --check apps/stellar-dashboard/src/main.mjs`
- Result: passed
- Command: `node --check apps/stellar-dashboard/src/dashboardModel.mjs`
- Result: passed
- Command: `node --check apps/stellar-dashboard/dashboardModel.test.mjs`
- Result: passed
- Command: `git diff --check`
- Result: passed

## Residual Risk

- No signing, transaction submission, wallet seeding, or on-chain state mutation was performed during Freighter testing.
- The connected Freighter account currently resolves to a fallback presentation rather than wallet-specific live position data.
- Production-like origin behavior is verified for read-only load, RPC, hash navigation, static fallback copy, and T3 gate rendering.
- Manual Freighter origin-permission approval remains human-gated and was not accepted by automation.

## Submit Gate

Proceed with T3 submission only if the reviewer accepts `Position Adapter: fallback` for an unseeded connected Freighter wallet and accepts the read-only P4 production-like origin check. Hold final submission if the acceptance criterion requires `Position Adapter: live` or a human-approved Freighter origin-permission pass from the production-like origin.
