#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

NETWORK="${NETWORK:-testnet}"
FRIENDBOT_URL="${FRIENDBOT_URL:-https://friendbot.stellar.org}"
STELLAR_STATE_DIR="${STELLAR_STATE_DIR:-${TMPDIR:-/tmp}/helix-blend-testnet-probe-stellar}"
SOURCE_ALIAS="${SOURCE_ALIAS:-helix-blend-probe}"
PROBE_BUILD_DIR="${PROBE_BUILD_DIR:-${TMPDIR:-/tmp}/helix-blend-testnet-probe-contract}"

MOCK_ORACLE_WASM="$ROOT_DIR/target/wasm32v1-none/release/helix_mock_oracle.wasm"
WRAPPER_WASM="$ROOT_DIR/target/wasm32v1-none/release/helix_blend_oracle_adaptor.wasm"
POOL_WASM="$ROOT_DIR/reference/blend-contracts-v2/target/wasm32v1-none/release/pool.wasm"
BACKSTOP_WASM="$PROBE_BUILD_DIR/target/wasm32v1-none/release/helix_blend_testnet_probe_backstop.wasm"

PRICE="${PRICE:-2345678900}"
DECIMALS="${DECIMALS:-7}"
RESOLUTION="${RESOLUTION:-300}"
SUPPLY_AMOUNT="${SUPPLY_AMOUNT:-10000000}"
BORROW_AMOUNT="${BORROW_AMOUNT:-1000000}"

stellar_cmd() {
  stellar --config-dir "$STELLAR_STATE_DIR" "$@"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

step() {
  printf '\n==> %s\n' "$1"
}

fund_identity() {
  local identity="$1"
  local address

  address="$(stellar_cmd keys address "$identity")"
  curl -fsS "${FRIENDBOT_URL}/?addr=${address}" >/dev/null
}

build_probe_backstop() {
  rm -rf "$PROBE_BUILD_DIR"
  mkdir -p "$PROBE_BUILD_DIR/src"

  cat > "$PROBE_BUILD_DIR/Cargo.toml" <<'EOF'
[package]
name = "helix-blend-testnet-probe-backstop"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib"]
doctest = false

[dependencies]
soroban-sdk = "=22.0.7"
EOF

  cat > "$PROBE_BUILD_DIR/src/lib.rs" <<'EOF'
#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[derive(Clone)]
#[contracttype]
pub struct PoolBackstopData {
    pub tokens: i128,
    pub shares: i128,
    pub q4w_pct: i128,
    pub blnd: i128,
    pub usdc: i128,
    pub token_spot_price: i128,
}

#[contract]
pub struct ProbeBackstop;

#[contractimpl]
impl ProbeBackstop {
    pub fn pool_data(_env: Env, _pool: Address) -> PoolBackstopData {
        PoolBackstopData {
            tokens: 50_000_0000000,
            shares: 50_000_0000000,
            q4w_pct: 0,
            blnd: 500_001_0000000,
            usdc: 12_501_0000000,
            token_spot_price: 0_2500000,
        }
    }
}
EOF

  cargo rustc \
    --manifest-path "$PROBE_BUILD_DIR/Cargo.toml" \
    --crate-type=cdylib \
    --target=wasm32v1-none \
    --release >/dev/null
}

require_cmd cargo
require_cmd curl
require_cmd stellar

mkdir -p "$STELLAR_STATE_DIR"

step "Creating friendbot-funded testnet source in $STELLAR_STATE_DIR"
stellar_cmd keys generate --overwrite "$SOURCE_ALIAS" >/dev/null
fund_identity "$SOURCE_ALIAS"
SOURCE_ADDRESS="$(stellar_cmd keys address "$SOURCE_ALIAS")"
printf 'source_address: %s\n' "$SOURCE_ADDRESS"

step "Building Helix mock oracle and Blend wrapper"
stellar contract build --package helix-mock-oracle >/dev/null
stellar contract build --package helix-blend-oracle-adaptor >/dev/null

step "Building real Blend pool for wasm32v1-none"
cargo rustc \
  --manifest-path=reference/blend-contracts-v2/pool/Cargo.toml \
  --crate-type=cdylib \
  --target=wasm32v1-none \
  --release >/dev/null

step "Building probe-only mock backstop"
build_probe_backstop

step "Resolving native XLM SAC as the probe reserve asset"
if ASSET_ID="$(
  stellar_cmd contract asset deploy \
    --asset native \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK"
)"; then
  :
else
  ASSET_ID="$(stellar_cmd contract id asset --asset native --network "$NETWORK")"
fi
printf 'asset_id: %s\n' "$ASSET_ID"

step "Deploying Helix mock oracle"
MOCK_ORACLE_ID="$(
  stellar_cmd contract deploy \
    --wasm "$MOCK_ORACLE_WASM" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK" \
    -- \
    --admin "$SOURCE_ADDRESS"
)"
printf 'mock_oracle_id: %s\n' "$MOCK_ORACLE_ID"

NOW="$(date +%s)"
stellar_cmd contract invoke \
  --id "$MOCK_ORACLE_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  set_price \
  --asset "$ASSET_ID" \
  --price "$PRICE" \
  --timestamp "$NOW" \
  >/dev/null

stellar_cmd contract invoke \
  --id "$MOCK_ORACLE_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  set_decimals \
  --asset "$ASSET_ID" \
  --decimals "$DECIMALS" \
  >/dev/null

step "Deploying and initializing Helix Blend oracle wrapper"
WRAPPER_ID="$(
  stellar_cmd contract deploy \
    --wasm "$WRAPPER_WASM" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK"
)"
printf 'wrapper_id: %s\n' "$WRAPPER_ID"

stellar_cmd contract invoke \
  --id "$WRAPPER_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  initialize \
  --admin "$SOURCE_ADDRESS" \
  --helix_oracle "$MOCK_ORACLE_ID" \
  --base '{"Other":"USD"}' \
  --assets '[{"Stellar":"'"$ASSET_ID"'"}]' \
  --decimals "$DECIMALS" \
  --resolution "$RESOLUTION" \
  >/dev/null

WRAPPER_DECIMALS="$(
  stellar_cmd contract invoke \
    --id "$WRAPPER_ID" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK" \
    -- \
    decimals
)"
WRAPPER_PRICE="$(
  stellar_cmd contract invoke \
    --id "$WRAPPER_ID" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK" \
    -- \
    lastprice \
    --asset '{"Stellar":"'"$ASSET_ID"'"}'
)"
printf 'wrapper_decimals: %s\n' "$WRAPPER_DECIMALS"
printf 'wrapper_lastprice: %s\n' "$WRAPPER_PRICE"

step "Deploying probe backstop and real Blend pool"
BACKSTOP_ID="$(
  stellar_cmd contract deploy \
    --wasm "$BACKSTOP_WASM" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK"
)"
printf 'backstop_id: %s\n' "$BACKSTOP_ID"

POOL_ID="$(
  stellar_cmd contract deploy \
    --wasm "$POOL_WASM" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK" \
    -- \
    --admin "$SOURCE_ADDRESS" \
    --name '"helix-hsteth-live-probe"' \
    --oracle "$WRAPPER_ID" \
    --bstop_rate 1000000 \
    --max_positions 4 \
    --min_collateral 1 \
    --backstop_id "$BACKSTOP_ID" \
    --blnd_id "$ASSET_ID"
)"
printf 'pool_id: %s\n' "$POOL_ID"

POOL_CONFIG="$(
  stellar_cmd contract invoke \
    --id "$POOL_ID" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK" \
    -- \
    get_config
)"
printf 'pool_config: %s\n' "$POOL_CONFIG"

step "Adding reserve and activating the real Blend pool"
RESERVE_CONFIG='{ "c_factor": 7500000, "decimals": 7, "enabled": true, "index": 0, "l_factor": 7500000, "max_util": 9500000, "r_base": 100000, "r_one": 500000, "r_three": 15000000, "r_two": 5000000, "reactivity": 20, "supply_cap": "1000000000000000000", "util": 7500000 }'

stellar_cmd contract invoke \
  --id "$POOL_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  queue_set_reserve \
  --asset "$ASSET_ID" \
  --metadata "$RESERVE_CONFIG" \
  >/dev/null

stellar_cmd contract invoke \
  --id "$POOL_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  set_reserve \
  --asset "$ASSET_ID" \
  >/dev/null

stellar_cmd contract invoke \
  --id "$POOL_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  set_status \
  --pool_status 0 \
  >/dev/null

step "Submitting supply+borrow through real Blend pool"
REQUESTS='[{"address":"'"$ASSET_ID"'","amount":"'"$SUPPLY_AMOUNT"'","request_type":2},{"address":"'"$ASSET_ID"'","amount":"'"$BORROW_AMOUNT"'","request_type":4}]'

stellar_cmd contract invoke \
  --id "$POOL_ID" \
  --source "$SOURCE_ALIAS" \
  --network "$NETWORK" \
  -- \
  submit \
  --from "$SOURCE_ADDRESS" \
  --spender "$SOURCE_ADDRESS" \
  --to "$SOURCE_ADDRESS" \
  --requests "$REQUESTS"

POSITIONS="$(
  stellar_cmd contract invoke \
    --id "$POOL_ID" \
    --source "$SOURCE_ALIAS" \
    --network "$NETWORK" \
    -- \
    get_positions \
    --address "$SOURCE_ADDRESS"
)"
printf 'positions: %s\n' "$POSITIONS"

printf '\nBlend testnet oracle probe passed.\n'
