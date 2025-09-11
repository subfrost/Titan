#!/bin/bash
set -e
BLOCK_HEIGHT=849236
URL="https://mainnet.sandshrew.io/v2/lasereyes"
HEADER="Content-Type: application/json"

# Get the block hash
BLOCK_HASH_HEX=$(curl -s -X POST -H "$HEADER" --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getblockhash\",\"params\":[$BLOCK_HEIGHT]}" $URL | jq -r '.result')

# Get the block data
BLOCK_DATA_HEX=$(curl -s -X POST -H "$HEADER" --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getblock\",\"params\":[\"$BLOCK_HASH_HEX\", 0]}" $URL | jq -r '.result')

# Convert hex to binary and save to file
echo -n $BLOCK_DATA_HEX | xxd -r -p > crates/alkanes-indexer/src/tests/static/849236.txt