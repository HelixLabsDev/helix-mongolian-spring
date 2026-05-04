#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

require_cmd stellar

SOURCE_ACCOUNT="helix-deployer"
ADMIN_ADDRESS="$(stellar keys address "$SOURCE_ACCOUNT")"
AXELAR_GATEWAY="CB2JYOOZPHO43R57TC5PXV22QICKIDC5NKRF62BZG2J6JYFUIQPIAYY3"
AXELAR_GAS_SERVICE="CCLZOCGHHC6F6JCZHEUP53LDQHRBPPCNRYXOVFZFS3O63OGRC47CKCGV"
HSTETH_TOKEN="CC366YM6MJOISQSCUXBU3BCRNKVCDI7VOZT3SF7AJKL7ILTMXY3AGBJ2"
COLLATERAL_VAULT="CAGG2XJJJGTER3E5BP26FVI3QLT4QYKZT233ZSPUC5O573QRU3D2Y7TW"
DEFAULT_SOURCE_ADDRESS="0x5A33F35f4B02269107e60713bc2dAb970C741a0c"
SUPERSEDED_MIGRATORS=(
  "0xd94e95de7759e134fa150987514f7cfb50802984"
)
SOURCE_CHAIN="${SOURCE_CHAIN:-ethereum-sepolia}"
SOURCE_ADDRESS="${SOURCE_ADDRESS:-$DEFAULT_SOURCE_ADDRESS}"
WASM_PATH="target/wasm32v1-none/release/bridge_handler.optimized.wasm"

if [ -z "$ADMIN_ADDRESS" ]; then
  echo "ADMIN_ADDRESS could not be resolved from $SOURCE_ACCOUNT." >&2
  exit 1
fi

for SUPERSEDED_MIGRATOR in "${SUPERSEDED_MIGRATORS[@]}"; do
  if [[ "${SOURCE_ADDRESS:l}" == "${SUPERSEDED_MIGRATOR:l}" ]]; then
    echo "SOURCE_ADDRESS points to a superseded AxelarMigrator: $SOURCE_ADDRESS" >&2
    exit 1
  fi
done

BUILD_START="$(date +%s)"
stellar contract build --package bridge-handler --optimize
BUILD_END="$(date +%s)"
BUILD_DURATION="$((BUILD_END - BUILD_START))"

if [ ! -f "$WASM_PATH" ]; then
  WASM_PATH="target/wasm32-unknown-unknown/release/bridge_handler.optimized.wasm"
fi

if [ ! -f "$WASM_PATH" ]; then
  WASM_PATH="target/wasm32v1-none/release/bridge_handler.wasm"
fi

if [ ! -f "$WASM_PATH" ]; then
  WASM_PATH="target/wasm32-unknown-unknown/release/bridge_handler.wasm"
fi

if [ ! -f "$WASM_PATH" ]; then
  echo "Optimized wasm not found in expected target paths." >&2
  exit 1
fi

set +e
DEPLOY_OUTPUT="$(
  stellar contract deploy \
    --verbose \
    --wasm "$WASM_PATH" \
    --source-account "$SOURCE_ACCOUNT" \
    --network testnet \
    -- \
    --admin "$ADMIN_ADDRESS" \
    --gateway "$AXELAR_GATEWAY" \
    --gas_service "$AXELAR_GAS_SERVICE" \
    --token "$HSTETH_TOKEN" \
    --vault "$COLLATERAL_VAULT" \
    --source_chain "$SOURCE_CHAIN" \
    --source_address "$SOURCE_ADDRESS" \
    2>&1
)"
DEPLOY_STATUS="$?"
set -e

printf '%s\n' "$DEPLOY_OUTPUT"

if [ "$DEPLOY_STATUS" -ne 0 ]; then
  echo "stellar contract deploy failed." >&2
  exit "$DEPLOY_STATUS"
fi

CONTRACT_ID="$(printf '%s\n' "$DEPLOY_OUTPUT" | grep -Eo 'C[A-Z2-7]{55}' | tail -1 | tr -d '[:space:]')"
DEPLOY_TX_HASH="$(printf '%s\n' "$DEPLOY_OUTPUT" | sed -nE 's/.*([0-9a-fA-F]{64}).*/\1/p' | tail -1 | tr -d '[:space:]')"
LEDGER_SEQUENCE="$(printf '%s\n' "$DEPLOY_OUTPUT" | sed -nE 's/.*ledger[^0-9]*([0-9]+).*/\1/p' | tail -1 | tr -d '[:space:]')"

if [ -z "$CONTRACT_ID" ]; then
  echo "Failed to parse deployed contract ID from stellar CLI output." >&2
  exit 1
fi

printf '\n=== BRIDGE HANDLER DEPLOY ===\n'
printf 'Build Duration (s): %s\n' "$BUILD_DURATION"
printf 'WASM Path: %s\n' "$WASM_PATH"
printf 'Contract ID: %s\n' "$CONTRACT_ID"
printf 'Deploy Tx Hash: %s\n' "${DEPLOY_TX_HASH:-unavailable}"
printf 'Ledger Sequence: %s\n' "${LEDGER_SEQUENCE:-unavailable}"
printf 'Admin: %s\n' "$ADMIN_ADDRESS"
printf 'Gateway: %s\n' "$AXELAR_GATEWAY"
printf 'Gas Service: %s\n' "$AXELAR_GAS_SERVICE"
printf 'Token: %s\n' "$HSTETH_TOKEN"
printf 'Vault: %s\n' "$COLLATERAL_VAULT"
printf 'Source Chain: %s\n' "$SOURCE_CHAIN"
printf 'Source Address: %s\n' "$SOURCE_ADDRESS"
printf '=============================\n'
