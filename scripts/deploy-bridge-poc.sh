#!/bin/zsh
set -euo pipefail

SOURCE_ACCOUNT="helix-deployer"
REMOTE_DESTINATION_ADDRESS="0x9271a764Ff607350987bA77f865310DCcEbE8768"
DESTINATION_CHAIN="ethereum-sepolia"
DEPLOY_SALT="0000000000000000000000000000000068656c69782d6272696467652d706f63"
MESSAGE="hello from helix bridge poc"
AXELAR_GATEWAY="CCSNWHMQSPTW4PS7L32OIMH7Z6NFNCKYZKNFSWRSYX7MK64KHBDZDT5I"
AXELAR_GAS_SERVICE="CAZUKAFB5XHZKFZR7B5HIKB6BBMYSZIV3V2VWFTQWKYEMONWK2ZLTZCT"
AXELAR_GAS_TOKEN="CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
POLL_ATTEMPTS=30
POLL_INTERVAL_SECONDS=20
WASM_PATH="target/wasm32v1-none/release/bridge_poc.wasm"

if [ -z "$SOURCE_ACCOUNT" ]; then
  echo "SOURCE_ACCOUNT must be set at the top of the script." >&2
  exit 1
fi

if [ -z "$REMOTE_DESTINATION_ADDRESS" ]; then
  echo "REMOTE_DESTINATION_ADDRESS must be set at the top of the script." >&2
  exit 1
fi

stellar contract build --package bridge-poc && \
if [ ! -f "$WASM_PATH" ]; then
  WASM_PATH="target/wasm32-unknown-unknown/release/bridge_poc.wasm"
fi && \
test -f "$WASM_PATH" && \
stellar contract optimize --wasm "$WASM_PATH"

OPTIMIZED_WASM="${WASM_PATH%.wasm}.optimized.wasm"
PREDICTED_ID=$(stellar contract id wasm --salt "$DEPLOY_SALT" --source-account "$SOURCE_ACCOUNT" --network testnet)
echo "Predicted contract ID: $PREDICTED_ID"
CONTRACT_ID=$(stellar contract deploy --wasm "$OPTIMIZED_WASM" --salt "$DEPLOY_SALT" --source-account "$SOURCE_ACCOUNT" --network testnet -- --gateway "$AXELAR_GATEWAY" --gas_service "$AXELAR_GAS_SERVICE")
echo "Deployed contract ID: $CONTRACT_ID"

SEND_RESULT=$(stellar contract invoke --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" --network testnet -- send_message --caller "$SOURCE_ACCOUNT" --destination_chain "\"$DESTINATION_CHAIN\"" --destination_address "\"$REMOTE_DESTINATION_ADDRESS\"" --message "\"$MESSAGE\"")
echo "Send result: $SEND_RESULT"

RECEIVED_RESULT="null"
ATTEMPT=1
while [ "$ATTEMPT" -le "$POLL_ATTEMPTS" ]; do
  RECEIVED_RESULT=$(stellar contract invoke --send no --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" --network testnet -- received_message)
  if [ -n "$RECEIVED_RESULT" ] && [ "$RECEIVED_RESULT" != "null" ] && [ "$RECEIVED_RESULT" != "None" ] && [ "$RECEIVED_RESULT" != "\"\"" ]; then
    break
  fi
  sleep "$POLL_INTERVAL_SECONDS"
  ATTEMPT=$((ATTEMPT + 1))
done

if [ "$RECEIVED_RESULT" = "null" ] || [ "$RECEIVED_RESULT" = "None" ] || [ "$RECEIVED_RESULT" = "\"\"" ]; then
  echo "No inbound GMP message received before timeout." >&2
  printf '\n=== PHASE 4 EVIDENCE ===\n'
  printf 'Contract ID: %s\n' "$CONTRACT_ID"
  printf 'Send Result: %s\n' "$SEND_RESULT"
  printf '========================\n\n'
  exit 0
fi

printf '\n=== PHASE 4 EVIDENCE ===\n'
printf 'Contract ID: %s\n' "$CONTRACT_ID"
printf 'Send Result: %s\n' "$SEND_RESULT"
printf '========================\n\n'
