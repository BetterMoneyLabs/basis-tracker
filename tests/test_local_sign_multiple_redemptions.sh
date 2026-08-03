#!/usr/bin/env bash
# Thin wrapper for tests/test_local_sign_multiple_redemptions.py.
# Run from the project root with the required environment variables.
#
# Example:
#   ISSUER_PRIVATE_KEY=... ./tests/test_local_sign_multiple_redemptions.sh

set -euo pipefail

cd "$(dirname "$0")/.."

python3 tests/test_local_sign_multiple_redemptions.py "$@"
