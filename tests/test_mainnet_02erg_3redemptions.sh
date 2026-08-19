#!/usr/bin/env bash
# Wrapper for tests/test_mainnet_02erg_3redemptions.py.
# Run from the project root.
#
# Example:
#   ./tests/test_mainnet_02erg_3redemptions.sh
#
# Optional environment overrides:
#   WALLET_ADDRESS, NODE_URL, API_KEY, TRACKER_URL, RESERVE_NFT_ID,
#   CLI_BIN, WAIT_TIMEOUT

set -euo pipefail

cd "$(dirname "$0")/.."

python3 tests/test_mainnet_02erg_3redemptions.py "$@"
