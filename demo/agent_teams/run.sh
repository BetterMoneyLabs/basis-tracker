#!/usr/bin/env bash
# Basis Agent Teams Demo launcher
# Builds binaries, validates the Ergo node setup, starts the tracker server,
# and runs the Python orchestrator.
#
# Unlike demo/agent_coop, this demo needs a REAL Ergo node: the judge's prize
# is backed by an on-chain reserve and redeemed on-chain, and cross-team
# credit is collateralized >= 50% via the managers' reserves.
#
# Required environment variables:
#   TRACKER_NFT_ID        - NFT identifying the tracker instance (in the node wallet)
#   JUDGE_RESERVE_NFT_ID  - NFT for the judge's reserve   (in the node wallet)
#   ADAM_RESERVE_NFT_ID   - NFT for team Alpha manager's reserve
#   BELLA_RESERVE_NFT_ID  - NFT for team Beta manager's reserve
# Optional:
#   BASIS_NODE_URL        - default http://127.0.0.1:9053
#   BASIS_NODE_API_KEY    - node API key (required for wallet operations)
#   BASIS_SERVER_URL      - default http://127.0.0.1:3048
#
# Usage:
#   ./demo/agent_teams/run.sh           # full demo
#   ./demo/agent_teams/run.sh --check   # preflight only: verify node, wallet, NFTs

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SERVER_URL="${BASIS_SERVER_URL:-http://127.0.0.1:3048}"
NODE_URL="${BASIS_NODE_URL:-http://127.0.0.1:9053}"
NODE_API_KEY="${BASIS_NODE_API_KEY:-}"

# Minimum wallet balance: 0.30 ERG reserves + tracker box + fees.
MIN_BALANCE_NANOERG=320000000

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

node_curl() {
    # node_curl <path> — GET against the Ergo node with the API key header.
    local args=(-s)
    if [[ -n "$NODE_API_KEY" ]]; then
        args+=(-H "api_key: $NODE_API_KEY")
    fi
    curl "${args[@]}" "$NODE_URL$1"
}

check_env() {
    local missing=()
    for var in TRACKER_NFT_ID JUDGE_RESERVE_NFT_ID ADAM_RESERVE_NFT_ID BELLA_RESERVE_NFT_ID; do
        if [[ -z "${!var:-}" ]]; then
            missing+=("$var")
        fi
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required environment variables: ${missing[*]}"
        echo "See demo/agent_teams/README.md — the demo needs four NFTs in the node wallet."
        exit 1
    fi
}

preflight() {
    log_info "Preflight: checking Ergo node at $NODE_URL ..."

    local info
    if ! info=$(node_curl /info 2>/dev/null) || [[ -z "$info" ]]; then
        log_error "Ergo node unreachable at $NODE_URL (see docs/ergo_node_setup.md)."
        exit 1
    fi
    local height
    height=$(echo "$info" | python3 -c "import sys, json; print(json.load(sys.stdin).get('fullHeight', 0))")
    log_info "  node reachable, full height $height"

    local status
    if ! status=$(node_curl /wallet/status 2>/dev/null) || [[ -z "$status" ]]; then
        log_error "Node wallet API not answering (check BASIS_NODE_API_KEY)."
        exit 1
    fi
    local unlocked
    unlocked=$(echo "$status" | python3 -c "import sys, json; print(json.load(sys.stdin).get('isUnlocked', False))")
    if [[ "$unlocked" != "True" ]]; then
        log_error "Node wallet is locked — unlock it first (see docs/ergo_node_setup.md)."
        exit 1
    fi
    log_info "  wallet unlocked"

    local balances
    balances=$(node_curl /wallet/balances)
    echo "$balances" | python3 - "$MIN_BALANCE_NANOERG" \
        "$TRACKER_NFT_ID" "$JUDGE_RESERVE_NFT_ID" "$ADAM_RESERVE_NFT_ID" "$BELLA_RESERVE_NFT_ID" <<'EOF'
import json, sys

min_balance, *nfts = sys.argv[1:]
data = json.load(sys.stdin)
balance = data.get("balance", 0)
assets = data.get("assets", {}) or {}

ok = True
if balance < int(min_balance):
    print(f"[ERROR] wallet balance {balance/1e9:.4f} ERG < required {int(min_balance)/1e9:.2f} ERG "
          "(three reserves + tracker box + fees)")
    ok = False
else:
    print(f"[INFO]   wallet balance {balance/1e9:.4f} ERG — sufficient")

for nft in nfts:
    if assets.get(nft, 0) < 1:
        print(f"[ERROR] NFT {nft[:16]}... not found in the node wallet")
        ok = False
    else:
        print(f"[INFO]   NFT {nft[:16]}... present")

sys.exit(0 if ok else 1)
EOF
}

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        log_info "Stopping tracker server (pid $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

check_env
preflight

if [[ "${1:-}" == "--check" ]]; then
    log_info "Preflight passed. Re-run without --check to start the demo."
    exit 0
fi

log_info "Building basis_server, basis_mcp, and basis_cli..."
cargo build --release -p basis_server -p basis_mcp -p basis_cli

log_info "Cleaning previous demo state..."
rm -rf "$SCRIPT_DIR/data" "$SCRIPT_DIR/config"

log_info "Generating tracker configuration..."
mkdir -p "$SCRIPT_DIR/config"

# Generate a fresh demo tracker keypair; the secret key lets the server sign
# redemption transactions locally. Demo keys only — never reuse.
KEYPAIR_JSON=$("$PROJECT_ROOT/target/release/basis_cli" generate-keypair --json)
TRACKER_PUBKEY=$(echo "$KEYPAIR_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['public_key_hex'])")
TRACKER_SECRET=$(echo "$KEYPAIR_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['private_key_hex'])")

# Start scanning from the current height to avoid a full rescan.
START_HEIGHT=$(node_curl /info | python3 -c "import sys, json; print(max(0, json.load(sys.stdin).get('fullHeight', 1) - 5))")

# Hardcoded reserve contract P2S from the server defaults.
RESERVE_P2S="3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT"

# Write the server config relative to the demo directory.
cat > "$SCRIPT_DIR/config/basis.toml" <<EOF
[server]
host = "127.0.0.1"
port = 3048
data_dir = "data"

[ergo]
basis_reserve_contract_p2s = "$RESERVE_P2S"
tracker_nft_id = "$TRACKER_NFT_ID"
tracker_public_key = "$TRACKER_PUBKEY"
tracker_secret_key = "$TRACKER_SECRET"

[ergo.node]
start_height = $START_HEIGHT
node_url = "$NODE_URL"
api_key = "$NODE_API_KEY"
EOF

log_info "Checking tracker server at $SERVER_URL..."
if curl -s "$SERVER_URL/health" >/dev/null 2>&1; then
    log_error "A tracker server is already running at $SERVER_URL."
    log_error "This demo needs its own tracker (demo config + data dir). Stop it first."
    exit 1
fi

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

# Wait for the on-chain tracker box (auto-created on startup; needed for redemption).
log_info "Waiting for the on-chain tracker box (auto-created by the server)..."
TRACKER_BOX_WAIT="${BASIS_TRACKER_BOX_WAIT:-180}"
elapsed=0
while (( elapsed < TRACKER_BOX_WAIT )); do
    if curl -s "$SERVER_URL/tracker/latest-box-id" 2>/dev/null | grep -q '"tracker_box_id"'; then
        log_info "Tracker box confirmed on-chain."
        break
    fi
    sleep 5
    (( elapsed += 5 ))
done
if (( elapsed >= TRACKER_BOX_WAIT )); then
    log_warn "No tracker box after ${TRACKER_BOX_WAIT}s — the demo continues, but the"
    log_warn "on-chain redemption step will likely fail (see docs/TRACKER_BOX_SETUP.md)."
fi

log_info "Running agent teams scenario..."
AUTO_FLAG=""
if [[ ! -t 0 ]]; then
    AUTO_FLAG="--auto"
fi
BASIS_MCP="$PROJECT_ROOT/target/release/basis-mcp" \
    BASIS_SERVER_URL="$SERVER_URL" \
    JUDGE_RESERVE_NFT_ID="$JUDGE_RESERVE_NFT_ID" \
    ADAM_RESERVE_NFT_ID="$ADAM_RESERVE_NFT_ID" \
    BELLA_RESERVE_NFT_ID="$BELLA_RESERVE_NFT_ID" \
    python3 "$SCRIPT_DIR/orchestrator.py" $AUTO_FLAG

log_info "Demo finished successfully."
