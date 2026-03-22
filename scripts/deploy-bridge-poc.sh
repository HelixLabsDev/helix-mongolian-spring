#!/bin/zsh
set -euo pipefail

SOURCE_ACCOUNT=""
REMOTE_DESTINATION_ADDRESS=""
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
CONTRACT_ID=$(stellar contract id wasm --salt "$DEPLOY_SALT" --source-account "$SOURCE_ACCOUNT" --network testnet)
DEPLOY_XDR=$(stellar contract deploy --build-only --wasm "$OPTIMIZED_WASM" --salt "$DEPLOY_SALT" --source-account "$SOURCE_ACCOUNT" --network testnet -- --gateway "$AXELAR_GATEWAY" --gas_service "$AXELAR_GAS_SERVICE")
DEPLOY_TX_HASH=$(stellar tx hash --network testnet "$DEPLOY_XDR")
DEPLOY_RESULT=$(stellar tx send --network testnet "$DEPLOY_XDR")

SEND_XDR=$(stellar contract invoke --build-only --id "$CONTRACT_ID" --source-account "$SOURCE_ACCOUNT" --network testnet -- send_message --caller "$SOURCE_ACCOUNT" --destination_chain "$DESTINATION_CHAIN" --destination_address "$REMOTE_DESTINATION_ADDRESS" --message "$MESSAGE")
SEND_TX_HASH=$(stellar tx hash --network testnet "$SEND_XDR")
SEND_RESULT=$(stellar tx send --network testnet "$SEND_XDR")

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
  echo "Track the outbound message on Axelarscan with tx hash: $SEND_TX_HASH" >&2
  exit 1
fi

printf 'Contract ID: %s\n' "$CONTRACT_ID"
printf 'Axelar Gateway: %s\n' "$AXELAR_GATEWAY"
printf 'Axelar Gas Service: %s\n' "$AXELAR_GAS_SERVICE"
printf 'Axelar Gas Token: %s\n' "$AXELAR_GAS_TOKEN"
printf 'Destination Chain: %s\n' "$DESTINATION_CHAIN"
printf 'Remote Destination Address: %s\n' "$REMOTE_DESTINATION_ADDRESS"
printf 'Deploy Tx Hash: %s\n' "$DEPLOY_TX_HASH"
printf 'Send Tx Hash: %s\n' "$SEND_TX_HASH"
printf 'Received Message: %s\n' "$RECEIVED_RESULT"
printf 'Deploy Result: %s\n' "$DEPLOY_RESULT"
printf 'Send Result: %s\n' "$SEND_RESULT"
