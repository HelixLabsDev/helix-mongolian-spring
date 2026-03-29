# Helix Liquidation Bot

This is a standalone Rust binary that monitors Helix vault positions off-chain and submits liquidation transactions through Stellar RPC. It is not a Soroban contract and it is intentionally not part of the workspace members in the repo root.

## What It Does

The bot runs a simple pipeline:

1. `scanner`: poll vault events and maintain an in-memory index of positions to watch
2. `health`: simulate `get_health_factor(user)` and read oracle prices for tracked positions
3. `executor`: simulate `liquidate(liquidator, user, repay_amount)` and only submit if simulation succeeds

The bot uses timeout + retry logic for every RPC call and will never submit a liquidation without a prior simulation.

## Build

```bash
cd liquidation-bot
cargo build
```

## Configure

Copy the example file and update the contract IDs:

```bash
cp config.example.toml config.toml
```

Set the liquidator secret key in the environment:

```bash
export LIQUIDATOR_SECRET_KEY="S..."
```

## Run

```bash
cd liquidation-bot
cargo run -- --config config.toml
```

## Configuration Fields

- `rpc_url`: Stellar RPC URL
- `network_passphrase`: network passphrase used for signing
- `vault_contract_id`: Helix vault contract ID
- `oracle_contract_id`: Helix oracle adaptor contract ID
- `token_contract_id`: borrow-token contract ID used to repay liquidations
- `poll_interval_secs`: seconds between main-loop iterations
- `min_profit_threshold`: minimum expected liquidation profit
- `max_liquidations_per_run`: cap per loop iteration
- `gas_budget`: initial fee budget before `prepare_transaction`

## Environment Variables

- `LIQUIDATOR_SECRET_KEY`: required; loaded from environment only

## Architecture

- `src/config.rs`: TOML config loading + secret key env handling
- `src/scanner.rs`: event polling and tracked-position index
- `src/health.rs`: health-factor evaluation and oracle reads
- `src/executor.rs`: simulation-first liquidation submission
- `src/rpc.rs`: Soroban/Stellar RPC wrapper with timeout + retry logic
- `src/main.rs`: CLI, logging, main loop, shutdown handling
