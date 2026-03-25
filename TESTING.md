# Testing

## Overview

Helix brings cross-chain LST collateral to Stellar via Soroban smart contracts. T1 scope is the core contract infrastructure only: token, vault, oracle adaptor, bridge proof-of-concept, and a mock bridge placeholder. The deployed testnet contracts are not initialized, so testnet verification is limited to confirming deployment existence on-chain; functional coverage comes from local tests.

## Prerequisites

- Rust 1.84.0 or newer
- `wasm32v1-none` target
- Stellar CLI v25.2.0

```sh
rustup target add wasm32v1-none
cargo install --locked stellar-cli --version 25.2.0
```

## Running Tests Locally

```sh
cargo test --all
```

`contractimport!` dependencies require the Wasm artifacts to exist first. Build the workspace Wasm, then run the test suite:

```sh
cargo build --target wasm32v1-none --release
cargo test --all
```

## Testnet Contract Addresses

| Contract | Contract ID | Deploy Tx |
| --- | --- | --- |
| Token (`hstETH`) | `CC366YM6MJOISQSCUXBU3BCRNKVCDI7VOZT3SF7AJKL7ILTMXY3AGBJ2` | [6d575fcf2fd3e27f067dca907dedddb799a69b7264c34b4c26a9f7df61bd02e5](https://stellar.expert/explorer/testnet/tx/6d575fcf2fd3e27f067dca907dedddb799a69b7264c34b4c26a9f7df61bd02e5) |
| Vault | `CAGG2XJJJGTER3E5BP26FVI3QLT4QYKZT233ZSPUC5O573QRU3D2Y7TW` | [3a8302845b65734e626345683b15e8524005d4284e7b1257459ddeda5a58cf6b](https://stellar.expert/explorer/testnet/tx/3a8302845b65734e626345683b15e8524005d4284e7b1257459ddeda5a58cf6b) |
| Oracle Adaptor | `CAKMPB4I5AYS76HT652WU4X3SQXWRSDOKNOQYUFWJU6MUOZAW6KLXCD5` | [671df682929adbd38e0f4798bf930a8ef0086403722c8e922d40d7ad8085b707](https://stellar.expert/explorer/testnet/tx/671df682929adbd38e0f4798bf930a8ef0086403722c8e922d40d7ad8085b707) |
| Bridge PoC | `CDDTQAJBXS62HUHXTBONCSLVWHX46QDDOEPVX6MQG6B4VXN67DYF7CAZ` | [d7c601add9cfa22fa07221bf6ca6ee398b973ec20ae45dce9599603655c109a1](https://stellar.expert/explorer/testnet/tx/d7c601add9cfa22fa07221bf6ca6ee398b973ec20ae45dce9599603655c109a1) |

## Contract Architecture

- Token: SEP-41 vault receipt token (`hstETH`) with vault-controlled mint and burn.
- Vault: collateral positions, health factor checks, Dutch Auction liquidation, and RBAC.
- Oracle Adaptor: SEP-40 interface with Reflector and DIA sources, TWAP, and deviation-triggered Safe Mode.
- Bridge PoC: Axelar GMP integration demo contract.
- Mock Bridge: T2 placeholder.

## Verifying Deployments On-Chain

Use the contract ID to confirm the deployed Wasm exists on testnet:

```sh
stellar contract info wasm-hash --id <CONTRACT_ID> --network testnet
```

This verifies deployment presence only. Initialization-dependent flows are not available on testnet yet.

## CI

[GitHub Actions](https://github.com/HelixLabsDev/helix-mongolian-spring/actions)
