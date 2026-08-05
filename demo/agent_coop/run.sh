#!/usr/bin/env bash
# Basis Agent Co-op Demo launcher
# Builds binaries, starts the tracker server, and runs the Python orchestrator.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SERVER_URL="${BASIS_SERVER_URL:-http://127.0.0.1:3048}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        log_info "Stopping tracker server (pid $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

log_info "Building basis_server, basis_mcp, and basis_cli..."
cargo build --release -p basis_server -p basis_mcp -p basis_cli

log_info "Cleaning previous demo state..."
rm -rf "$SCRIPT_DIR/data" "$SCRIPT_DIR/config" "$SCRIPT_DIR/crates"

log_info "Generating tracker configuration..."
mkdir -p "$SCRIPT_DIR/config"

# Generate a fresh tracker keypair for the demo.
KEYPAIR_JSON=$("$PROJECT_ROOT/target/release/basis_cli" generate-keypair --json)
TRACKER_PUBKEY=$(echo "$KEYPAIR_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['public_key_hex'])")

# Random 32-byte tracker NFT ID (hex). It does not need to exist on-chain for
# the pure-credit demo, but the server requires a validly formatted value.
TRACKER_NFT_ID=$(python3 -c "import secrets; print(secrets.token_hex(32))")

# Hardcoded reserve contract P2S from the server defaults; only used to satisfy
# config validation in this no-reserve demo.
RESERVE_P2S="3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT"

# Write a minimal server config relative to the demo directory.
cat > "$SCRIPT_DIR/config/basis.toml" <<EOF
[server]
host = "127.0.0.1"
port = 3048
data_dir = "data"

[ergo]
basis_reserve_contract_p2s = "$RESERVE_P2S"
tracker_nft_id = "$TRACKER_NFT_ID"
tracker_public_key = "$TRACKER_PUBKEY"

[ergo.node]
start_height = 0
node_url = "http://127.0.0.1:9053"
EOF

log_info "Checking tracker server at $SERVER_URL..."
if curl -s "$SERVER_URL/health" >/dev/null 2>&1; then
    log_warn "A tracker server is already running at $SERVER_URL; reusing it."
else
    log_info "Starting tracker server..."
    (
        cd "$SCRIPT_DIR"
        BASIS_SERVER_URL="$SERVER_URL" exec "$PROJECT_ROOT/target/release/basis_server"
    ) &
    SERVER_PID=$!

    # Wait for server to be ready.
    for i in {1..30}; do
        if curl -s "$SERVER_URL/health" >/dev/null 2>&1; then
            log_info "Tracker server is ready."
            break
        fi
        sleep 0.5
    done

    if ! curl -s "$SERVER_URL/health" >/dev/null 2>&1; then
        log_error "Tracker server failed to start."
        exit 1
    fi
fi

log_info "Running agent co-op scenario..."
BASIS_MCP="$PROJECT_ROOT/target/release/basis-mcp" \
    BASIS_SERVER_URL="$SERVER_URL" \
    python3 "$SCRIPT_DIR/orchestrator.py"

log_info "Demo finished successfully."
