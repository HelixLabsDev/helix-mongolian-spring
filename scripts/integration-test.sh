#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

NETWORK="testnet"
RPC_URL="https://soroban-testnet.stellar.org"
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
FRIENDBOT_URL="https://friendbot.stellar.org"
STELLAR_STATE_DIR="${TMPDIR:-/tmp}/helix-liquidation-testnet-stellar"
BOT_CONFIG_PATH="$ROOT_DIR/liquidation-bot/config.testnet.toml"

ALICE="alice"
BOB="bob"
CHARLIE="charlie"

MOCK_ORACLE_WASM="$ROOT_DIR/target/wasm32v1-none/release/helix_mock_oracle.wasm"
TOKEN_WASM="$ROOT_DIR/target/wasm32v1-none/release/helix_token.wasm"
VAULT_WASM="$ROOT_DIR/target/wasm32v1-none/release/helix_vault.wasm"

POOL_CONFIG_JSON='{"max_ltv":7500,"liq_threshold":8000,"liq_bonus":500,"interest_rate":500,"min_position":"1000000"}'
INITIAL_PRICE=2500000000
LIQUIDATION_PRICE=1500000000
COLLATERAL_SHARES=10000000
BORROW_AMOUNT=1500000000
VAULT_LIQUIDITY=5000000000
POLL_INTERVAL_SECS=10
MAX_LIQUIDATIONS_PER_RUN=5
MIN_PROFIT_THRESHOLD=0
GAS_BUDGET=200000

export STELLAR_CONFIG_DIR="$STELLAR_STATE_DIR"

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

  address="$(stellar keys address "$identity")"
  curl -fsS "${FRIENDBOT_URL}/?addr=${address}" >/dev/null
}

latest_ledger_sequence() {
  stellar ledger latest --network "$NETWORK" --output json | jq -er '.sequence // .result.sequence'
}

current_timestamp() {
  date +%s
}

require_cmd cargo
require_cmd curl
require_cmd jq
require_cmd stellar

mkdir -p "$STELLAR_CONFIG_DIR"

step "Setting up testnet identities in $STELLAR_CONFIG_DIR"
for identity in "$ALICE" "$BOB" "$CHARLIE"; do
  stellar keys generate --overwrite "$identity" >/dev/null
  fund_identity "$identity"
done

ALICE_ADDRESS="$(stellar keys address "$ALICE")"
BOB_ADDRESS="$(stellar keys address "$BOB")"
CHARLIE_ADDRESS="$(stellar keys address "$CHARLIE")"

printf 'alice:   %s\n' "$ALICE_ADDRESS"
printf 'bob:     %s\n' "$BOB_ADDRESS"
printf 'charlie: %s\n' "$CHARLIE_ADDRESS"

step "Building mock-oracle, token, and vault contracts"
stellar contract build --package helix-mock-oracle
stellar contract build --package helix-token
stellar contract build --package helix-vault

step "Deploying mock oracle"
ORACLE_ID="$(
  stellar contract deploy \
    --wasm "$MOCK_ORACLE_WASM" \
    --source "$ALICE" \
    --network "$NETWORK" \
    -- \
    --admin "$ALICE_ADDRESS"
)"
printf 'oracle_id: %s\n' "$ORACLE_ID"

step "Wrapping native XLM as the borrow token SAC"
if BORROW_TOKEN_ID="$(
  stellar contract asset deploy \
    --asset native \
    --source "$ALICE" \
    --network "$NETWORK"
)"; then
  :
else
  BORROW_TOKEN_ID="$(stellar contract id asset --asset native --network "$NETWORK")"
fi
printf 'borrow_token_id: %s\n' "$BORROW_TOKEN_ID"

step "Deploying vault"
VAULT_ID="$(
  stellar contract deploy \
    --wasm "$VAULT_WASM" \
    --source "$ALICE" \
    --network "$NETWORK"
)"
printf 'vault_id: %s\n' "$VAULT_ID"

step "Initializing vault"
stellar contract invoke \
  --id "$VAULT_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  initialize \
  --admin "$ALICE_ADDRESS" \
  --oracle "$ORACLE_ID" \
  --borrow_token "$BORROW_TOKEN_ID" \
  --config "$POOL_CONFIG_JSON" \
  >/dev/null

step "Deploying token"
TOKEN_ID="$(
  stellar contract deploy \
    --wasm "$TOKEN_WASM" \
    --source "$ALICE" \
    --network "$NETWORK"
)"
printf 'token_id: %s\n' "$TOKEN_ID"

step "Initializing token"
stellar contract invoke \
  --id "$TOKEN_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  initialize \
  --admin "$ALICE_ADDRESS" \
  --vault "$VAULT_ID" \
  --bridge "$ALICE_ADDRESS" \
  --name "Helix Staked ETH" \
  --symbol "hstETH" \
  --decimals 7 \
  >/dev/null

step "Registering collateral and liquidator role"
stellar contract invoke \
  --id "$VAULT_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  add_supported_asset \
  --asset "$TOKEN_ID" \
  >/dev/null

stellar contract invoke \
  --id "$VAULT_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  grant_role \
  --addr "$CHARLIE_ADDRESS" \
  --role 1 \
  >/dev/null

step "Seeding Bob's collateral position"
NOW="$(current_timestamp)"
stellar contract invoke \
  --id "$ORACLE_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  set_price \
  --asset "$TOKEN_ID" \
  --price "$INITIAL_PRICE" \
  --timestamp "$NOW" \
  >/dev/null

stellar contract invoke \
  --id "$ORACLE_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  set_decimals \
  --asset "$TOKEN_ID" \
  --decimals 7 \
  >/dev/null

stellar contract invoke \
  --id "$TOKEN_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  bridge_mint \
  --to "$BOB_ADDRESS" \
  --shares "$COLLATERAL_SHARES" \
  >/dev/null

step "Setting exchange rate for collateral token"
stellar contract invoke \
  --id "$TOKEN_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  update_exchange_rate \
  --new_total_assets "$COLLATERAL_SHARES" \
  >/dev/null

APPROVAL_LEDGER="$(( $(latest_ledger_sequence) + 1000 ))"
stellar contract invoke \
  --id "$TOKEN_ID" \
  --source "$BOB" \
  --network "$NETWORK" \
  -- \
  approve \
  --from "$BOB_ADDRESS" \
  --spender "$VAULT_ID" \
  --amount "$COLLATERAL_SHARES" \
  --expiration_ledger "$APPROVAL_LEDGER" \
  >/dev/null

stellar contract invoke \
  --id "$VAULT_ID" \
  --source "$BOB" \
  --network "$NETWORK" \
  -- \
  deposit \
  --user "$BOB_ADDRESS" \
  --collateral_token "$TOKEN_ID" \
  --amount "$COLLATERAL_SHARES" \
  >/dev/null

step "Funding vault borrow liquidity with wrapped native XLM"
stellar contract invoke \
  --id "$BORROW_TOKEN_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  transfer \
  --from "$ALICE_ADDRESS" \
  --to "$VAULT_ID" \
  --amount "$VAULT_LIQUIDITY" \
  >/dev/null

step "Borrowing against the seeded position"
stellar contract invoke \
  --id "$VAULT_ID" \
  --source "$BOB" \
  --network "$NETWORK" \
  -- \
  borrow \
  --user "$BOB_ADDRESS" \
  --amount "$BORROW_AMOUNT" \
  >/dev/null

step "Dropping collateral price to make the position liquidatable"
NOW="$(current_timestamp)"
stellar contract invoke \
  --id "$ORACLE_ID" \
  --source "$ALICE" \
  --network "$NETWORK" \
  -- \
  set_price \
  --asset "$TOKEN_ID" \
  --price "$LIQUIDATION_PRICE" \
  --timestamp "$NOW" \
  >/dev/null

step "Writing liquidation bot config"
DEPLOY_LEDGER="$(latest_ledger_sequence)"
cat >"$BOT_CONFIG_PATH" <<EOF
rpc_url = "$RPC_URL"
network_passphrase = "$NETWORK_PASSPHRASE"
vault_contract_id = "$VAULT_ID"
oracle_contract_id = "$ORACLE_ID"
token_contract_id = "$BORROW_TOKEN_ID"
start_ledger = $DEPLOY_LEDGER
poll_interval_secs = $POLL_INTERVAL_SECS
min_profit_threshold = $MIN_PROFIT_THRESHOLD
max_liquidations_per_run = $MAX_LIQUIDATIONS_PER_RUN
gas_budget = $GAS_BUDGET
EOF

step "Deployment summary"
printf 'oracle_id:       %s\n' "$ORACLE_ID"
printf 'borrow_token_id: %s\n' "$BORROW_TOKEN_ID"
printf 'vault_id:        %s\n' "$VAULT_ID"
printf 'token_id:        %s\n' "$TOKEN_ID"
printf 'bot config:      %s\n' "$BOT_CONFIG_PATH"

step "Run the liquidation bot"
printf 'export STELLAR_CONFIG_DIR="%s"\n\n' "$STELLAR_CONFIG_DIR"
cat <<EOF
LIQUIDATOR_SECRET_KEY="\$(stellar keys secret $CHARLIE)" \\
  cargo run --manifest-path liquidation-bot/Cargo.toml -- \\
  --config liquidation-bot/config.testnet.toml
EOF

step "Verify Bob's position after liquidation"
cat <<EOF
stellar contract invoke --id "$VAULT_ID" --source $ALICE --network testnet \\
  -- get_position --user "$BOB_ADDRESS"
EOF
