#!/usr/bin/env bash
# Wrapper for tests/test_mainnet_04erg_3redemptions_restart.py.
# Run from the project root.
#
# Example:
#   ./tests/test_mainnet_04erg_3redemptions_restart.sh
#
# Optional environment overrides:
#   WALLET_ADDRESS, NODE_URL, API_KEY, TRACKER_URL, RESERVE_NFT_ID,
#   RESERVE_AMOUNT, NOTE_AMOUNT, REDEEM_AMOUNT, NUM_REDEMPTIONS,
#   FEE_BOX_AMOUNT, CLI_BIN, WAIT_TIMEOUT,
#   SERVER_START_SCRIPT, SERVER_STOP_SCRIPT

set -euo pipefail

cd "$(dirname "$0")/.."

python3 tests/test_mainnet_04erg_3redemptions_restart.py "$@"
