#!/usr/bin/env bash
# Basis + Celaut + USE Stablecoin Demo launcher
#
# Shows agentic credit (pure credit + collateralized credit) and on-chain
# redemption of USE-token-backed IOU notes against a real Ergo node.
#
# Required environment variables:
#   USE_TOKEN_ID          - hex-encoded token id of the USE stablecoin (64 chars)
#   DAVE_RESERVE_NFT_ID   - NFT for user_dave's USE-backed reserve (in node wallet)
#   TRACKER_NFT_ID        - NFT identifying the tracker instance (in node wallet)
#
# Optional:
#   BASIS_NODE_URL        - default http://127.0.0.1:9053
#   BASIS_NODE_API_KEY    - node API key (required for wallet operations)
#   BASIS_SERVER_URL      - default http://127.0.0.1:3048
#
# Usage:
#   ./demo/agent_celaut_use/run.sh           # full demo
#   ./demo/agent_celaut_use/run.sh --check   # preflight only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SERVER_URL="${BASIS_SERVER_URL:-http://127.0.0.1:3048}"
NODE_URL="${BASIS_NODE_URL:-http://127.0.0.1:9053}"
NODE_API_KEY="${BASIS_NODE_API_KEY:-}"

USE_TOKEN_ID="${USE_TOKEN_ID:-}"
DAVE_RESERVE_NFT_ID="${DAVE_RESERVE_NFT_ID:-}"
TRACKER_NFT_ID="${TRACKER_NFT_ID:-}"

# USE has 3 decimals in this deployment.
USE_DECIMALS=3
USE_UNIT=$((10 ** USE_DECIMALS))

# Minimum wallet balance: tracker box + reserve storage rent + fees.
# Dave's reserve needs some ERG for storage rent and >= 0.5 USE (500 raw units).
MIN_BALANCE_NANOERG=50000000   # 0.05 ERG
MIN_USE_UNITS=$((5 * USE_UNIT / 10))  # 0.5 USE = 500 raw units

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

node_curl() {
    local args=(-s)
    if [[ -n "$NODE_API_KEY" ]]; then
        args+=(-H "api_key: $NODE_API_KEY")
    fi
    curl "${args[@]}" "$NODE_URL$1"
}

node_post_json() {
    local path="$1"
    local body="$2"
    local args=(-s -H "Content-Type: application/json")
    if [[ -n "$NODE_API_KEY" ]]; then
        args+=(-H "api_key: $NODE_API_KEY")
    fi
    curl "${args[@]}" -d "$body" "$NODE_URL$path"
}

wait_for_tx_confirm() {
    local txid="$1"
    local label="${2:-transaction}"
    log_info "  Waiting for $label $txid to confirm..."
    for i in {1..60}; do
        if node_curl "/blockchain/transaction/byId/${txid}" >/dev/null 2>&1; then
            log_info "  $label confirmed"
            return 0
        fi
        sleep 3
    done
    log_error "  $label $txid did not confirm in time"
    return 1
}

ensure_fee_box() {
    local min_value=2000000  # 0.002 ERG, enough to cover a 0.001 ERG fee
    log_info "Preflight: ensuring a token-free ERG fee box exists..."
    local boxes
    boxes=$(node_curl "/wallet/boxes/unspent")
    local plain_box
    plain_box=$(echo "$boxes" | python3 -c "
import json, sys
boxes = json.load(sys.stdin)
for entry in boxes:
    box = entry.get('box', entry)
    if not box.get('assets') and box.get('value', 0) >= $min_value:
        print(box['boxId'])
        break
")
    if [[ -n "$plain_box" ]]; then
        log_info "  Token-free fee box already present"
        return 0
    fi

    log_warn "  No token-free fee box found; creating one from a wallet box"
    local txid
    txid=$(NODE_URL="$NODE_URL" NODE_API_KEY="$NODE_API_KEY" \
        TRACKER_NFT_ID="$TRACKER_NFT_ID" DAVE_RESERVE_NFT_ID="$DAVE_RESERVE_NFT_ID" \
        python3 - <<'PY'
import json, os, sys, urllib.request, urllib.error

node_url = os.environ["NODE_URL"].rstrip("/")
api_key = os.environ.get("NODE_API_KEY", "")
tracker_nft_id = os.environ["TRACKER_NFT_ID"]
reserve_nft_id = os.environ["DAVE_RESERVE_NFT_ID"]

def req(method, path, body=None):
    headers = {}
    if api_key:
        headers["api_key"] = api_key
    data = json.dumps(body).encode() if body is not None else None
    if data is not None:
        headers["Content-Type"] = "application/json"
    r = urllib.request.Request(node_url + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(r, timeout=30) as resp:
            return resp.status, json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode(errors="replace"))

status, wallet_boxes = req("GET", "/wallet/boxes/unspent")
if status != 200:
    sys.exit(f"/wallet/boxes/unspent failed ({status}): {wallet_boxes}")

prohibited_token_ids = {tracker_nft_id, reserve_nft_id}

def box_tokens(box):
    return {a["tokenId"] for a in box.get("assets", [])}

def has_prohibited_token(box):
    return bool(box_tokens(box) & prohibited_token_ids)

fee = 1_000_000
fee_box_value = 50_000_000

# Prefer a box that already has no tokens and enough ERG.  If none exists,
# fall back to any wallet box with enough ERG that does not contain the
# tracker or reserve NFTs.
candidates = []
for entry in wallet_boxes:
    box = entry.get("box", entry)
    if has_prohibited_token(box):
        continue
    if box.get("value", 0) < fee_box_value + fee:
        continue
    candidates.append(box)

plain_candidates = [b for b in candidates if not box_tokens(b)]
chosen = plain_candidates[0] if plain_candidates else (candidates[0] if candidates else None)
if not chosen:
    sys.exit("no suitable wallet box to create a fee box (need a box without tracker/reserve NFTs and >= 0.051 ERG)")

status, addresses = req("GET", "/wallet/addresses")
wallet_address = addresses[0] if status == 200 and addresses else None
if not wallet_address:
    sys.exit("wallet has no addresses")

status, tree_resp = req("GET", f"/script/addressToTree/{wallet_address}")
wallet_tree = tree_resp.get("tree") if status == 200 and isinstance(tree_resp, dict) else str(tree_resp)
if not wallet_tree:
    sys.exit("could not convert wallet address to ergoTree")

current_height = req("GET", "/info")[1]["fullHeight"]

inputs = [{"boxId": chosen["boxId"], "extension": {}}]
inputs_raw = [req("GET", f"/utxo/byIdBinary/{chosen['boxId']}")[1]["bytes"]]

outputs = [
    {
        "value": fee_box_value,
        "ergoTree": wallet_tree,
        "creationHeight": current_height,
        "assets": [],
        "additionalRegisters": {},
    },
    {
        "value": chosen["value"] - fee_box_value - fee,
        "ergoTree": wallet_tree,
        "creationHeight": current_height,
        "assets": chosen.get("assets", []),
        "additionalRegisters": {},
    },
    {
        "value": fee,
        "ergoTree": "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304",
        "creationHeight": current_height,
        "assets": [],
        "additionalRegisters": {},
    },
]

unsigned = {
    "tx": {"inputs": inputs, "dataInputs": [], "outputs": outputs},
    "inputsRaw": inputs_raw,
    "dataInputsRaw": [],
    "secrets": {"dlog": []},
}

status, signed = req("POST", "/wallet/transaction/sign", unsigned)
if status != 200:
    sys.exit(f"/wallet/transaction/sign failed ({status}): {signed}")

status, txid = req("POST", "/transactions", signed)
if status != 200:
    sys.exit(f"/transactions broadcast failed ({status}): {txid}")
print(txid)
PY
)
    if [[ ${#txid} -ne 64 ]] || ! [[ "$txid" =~ ^[0-9a-fA-F]+$ ]]; then
        log_error "  Fee box creation failed: $txid"
        return 1
    fi
    log_info "  Fee box creation tx: $txid"
    wait_for_tx_confirm "$txid" "fee box"
}

ensure_use_reserve() {
    log_info "Preflight: ensuring Dave's USE-backed reserve exists before starting server..."
    # Lock the reserve NFT into the token reserve contract before the tracker
    # server starts.  Otherwise the tracker's box updater may select the
    # unspent NFT box as a fee input.
    if ! DAVE_PUBKEY="${DAVE_PUBKEY:-$DEFAULT_DAVE_PUBKEY}" \
         DAVE_RESERVE_AMOUNT="$MIN_USE_UNITS" \
         python3 "$SCRIPT_DIR/reserve_helper.py"; then
        log_error "Reserve preflight failed"
        exit 1
    fi
}

# Use a provided tracker keypair if available; otherwise fall back to the fixed
# demo keypair so the on-chain reserve / tracker box state stays consistent.
# The secret key lets the server sign tracker updates and redemption transactions.
# Demo keys only — never reuse outside of tests.
DEFAULT_TRACKER_PUBKEY="039aa1478e19ad14e55c51bd306514636c608b0236edffbf03ca4028c063c4c99b"
DEFAULT_TRACKER_SECRET="bd9c331161cb8432c4037c198e33deb77c99b2b36a6f7956be1d1e6f829c5eca"
DEFAULT_DAVE_PUBKEY="0278fc7226b1e34340709d55c088f5dc41b55426b10d2853ea8ed039d467e95c39"
DEFAULT_DAVE_SECRET="b584139c010b9e0178cd30cb8bc70e3995e99ca5551e5588439fdc7990fa55a7"

if [[ -n "${TRACKER_PUBKEY:-}" && -n "${TRACKER_SECRET:-}" ]]; then
    log_info "Using provided tracker keypair."
else
    TRACKER_PUBKEY="$DEFAULT_TRACKER_PUBKEY"
    TRACKER_SECRET="$DEFAULT_TRACKER_SECRET"
    log_info "Using fixed demo tracker keypair."
fi

if [[ -n "${DAVE_PUBKEY:-}" && -n "${DAVE_SECRET:-}" ]]; then
    log_info "Using provided Dave keypair."
else
    DAVE_PUBKEY="$DEFAULT_DAVE_PUBKEY"
    DAVE_SECRET="$DEFAULT_DAVE_SECRET"
    log_info "Using fixed demo Dave keypair."
fi

check_env() {
    local missing=()
    for var in USE_TOKEN_ID DAVE_RESERVE_NFT_ID TRACKER_NFT_ID; do
        if [[ -z "${!var:-}" ]]; then
            missing+=("$var")
        fi
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required environment variables: ${missing[*]}"
        echo "See demo/agent_celaut_use/README.md"
        exit 1
    fi

    if [[ ${#USE_TOKEN_ID} -ne 64 ]] || ! [[ "$USE_TOKEN_ID" =~ ^[0-9a-fA-F]+$ ]]; then
        log_error "USE_TOKEN_ID must be a 64-character hex string"
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
    BALANCE_JSON="$balances" MIN_BALANCE_NANO="$MIN_BALANCE_NANOERG" MIN_USE_UNITS="$MIN_USE_UNITS" \
        USE_TOKEN_ID_CHECK="$USE_TOKEN_ID" DAVE_NFT_CHECK="$DAVE_RESERVE_NFT_ID" \
        python3 -c "
import json, os, sys

data = json.loads(os.environ['BALANCE_JSON'])
balance = data.get('balance', 0)
assets = data.get('assets', {}) or {}
min_balance_nano = int(os.environ['MIN_BALANCE_NANO'])
min_use_units = int(os.environ['MIN_USE_UNITS'])
use_token_id = os.environ['USE_TOKEN_ID_CHECK']
dave_nft = os.environ['DAVE_NFT_CHECK']

ok = True
if balance < min_balance_nano:
    print(f'[ERROR] wallet balance {balance/1e9:.4f} ERG < required {min_balance_nano/1e9:.4f} ERG')
    ok = False
else:
    print(f'[INFO]   wallet balance {balance/1e9:.4f} ERG — sufficient')

use_balance = assets.get(use_token_id, 0)
if use_balance < min_use_units:
    print(f'[ERROR] USE token balance {use_balance} < required {min_use_units}')
    ok = False
else:
    print(f'[INFO]   USE token balance {use_balance} — sufficient')

if assets.get(dave_nft, 0) < 1:
    print(f'[WARN]   Dave reserve NFT {dave_nft[:16]}... not in wallet; will check on-chain')
else:
    print(f'[INFO]   Dave reserve NFT {dave_nft[:16]}... present in wallet')

sys.exit(0 if ok else 1)
"

    # The reserve NFT may already be locked in an on-chain reserve box (e.g. on a re-run).
    # Treat it as present if it exists in any unspent box.
    local dave_nft_boxes
    dave_nft_boxes=$(node_curl "/blockchain/box/unspent/byTokenId/${DAVE_RESERVE_NFT_ID}?limit=1")
    if [[ "$dave_nft_boxes" == "[]" ]]; then
        log_error "Dave reserve NFT $DAVE_RESERVE_NFT_ID not found on-chain or in wallet"
        exit 1
    fi
    log_info "  Dave reserve NFT $DAVE_RESERVE_NFT_ID present on-chain"
    # The tracker NFT must exist on-chain as a tracker box; it does not need to be
    # in the node wallet once the tracker box has been bootstrapped.
    local tracker_boxes
    tracker_boxes=$(node_curl "/blockchain/box/unspent/byTokenId/${TRACKER_NFT_ID}?limit=1")
    if [[ "$tracker_boxes" == "[]" ]]; then
        log_error "Tracker NFT $TRACKER_NFT_ID not found on-chain"
        exit 1
    fi
    log_info "  Tracker NFT $TRACKER_NFT_ID present on-chain"

    # Wallet address used for fee-input change outputs so fee boxes are recycled
    # back into the node wallet instead of the tracker key address.
    NODE_WALLET_ADDRESS=$(node_curl "/wallet/addresses" | python3 -c "import sys,json; print(json.load(sys.stdin)[0])")
    if [[ -z "$NODE_WALLET_ADDRESS" ]]; then
        log_error "Could not determine node wallet address"
        exit 1
    fi
    log_info "  Node wallet change address: $NODE_WALLET_ADDRESS"
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
ensure_fee_box
ensure_use_reserve

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

# Start scanning from a few blocks before the on-chain reserve (if one already
# exists) so the tracker picks it up.  Fall back to current height - 5.
CURRENT_HEIGHT=$(node_curl /info | python3 -c "import sys, json; print(json.load(sys.stdin).get('fullHeight', 1))")
RESERVE_HEIGHT=$(node_curl "/blockchain/box/unspent/byTokenId/${DAVE_RESERVE_NFT_ID}?limit=1" | \
    python3 -c "import sys, json; boxes=json.load(sys.stdin); print(boxes[0].get('creationHeight', 0) if boxes else 0)")
if [[ -n "$RESERVE_HEIGHT" && "$RESERVE_HEIGHT" -gt 0 ]]; then
    START_HEIGHT=$((RESERVE_HEIGHT - 10))
else
    START_HEIGHT=$((CURRENT_HEIGHT - 5))
fi
if [[ "$START_HEIGHT" -lt 0 ]]; then
    START_HEIGHT=0
fi

# ERG-backed reserve contract P2S from the server defaults.
RESERVE_P2S="3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT"

# Token-backed reserve contract P2S from the config example.
TOKEN_RESERVE_P2S="96HrjMftJd4NbjzufhMHXyZqzaUbdc5zUqtSySUnyEZoHogM9TD9RsRNkhq83tGaWTH4ZiFeDKzAdHkQyi1SWwMdkKtDaDhobhsrb5tjEDZkYhhF5aPGf3b4fUo3W23tec9N7GyzA6buyhRYzf3DqVWRsJGzCEdYeDfcMYKXon3wMxMFgQPX4FUbCAGe32dCd1aiFwQwx4Rnjy19G6qPaACdZvfSNFLgM1ur3U6HSSMX22g5o8MbbxzNeKScWFedW76z1zCmC38BrgiSv1qC7395yi9y4dDy2y26Tgrc2MPxvned5j1F3cTTTY9HVPYcS9U4vQocDRHufEkMG4dyu7eQqiHbqa6962B1jKQUMofyNV2mehQDrTzfzT5yHPsTeAGMTbDHeDECsmmJ2ideonha9VBuEP6fivixWjej43spbMZDcy3SCNa5gTrLuhh8gN4j8CbMncTXYghrxNdRqPbiUfJy8rMpcdeRaDnbodibGAyJqzzh95ZC6FHPwL7UQZbFbc472WSPtPRXU9g7QpZasdYtZb4GrHqvUJASsLwgYsRevJ4QZ24nrcUXv5ttziaVsGfeCW35viGB5zc3Sxk8e37smwQzWPfAGvsLfgy98dJfmNFyQUCNUyGX5qq4uJrKNaRamouL4S1VNFcgVaJTdGDTT7ykcy6qLmaFg6tUiAsLLC2p4GGB83fqsw13hfmxCxMGuFQUFu6aNc9W31wMMJ2beCN7Xb7cUkffzEXZnjiqFKMicTHsmvLz2mZuAgiKykrxqNTMqmTd2DjGsuYmQNtLXqS4j5d9qXnSVxmeVGp3cqcC7NYF9ujvLT38yqfnk5tZ5X9gnMXgkkKk8MfuUgY5a2ccSgbetkj7yNk1ciHK1QYGrvydDPWB2kJEQb9yAb72Uf1jZXVH1kSAr7vv1BBRQWveJcsjD6GQVKXJrLTGLrR5XV1uYK7uQSR4XYEi5yJsjk4rPMCDtt2FDsrUk7bo9suFWq2sqZxQZZWEgoKkwog6DtvyX4cyMpT45eeqjctBTjnAnJRf8CgBuyEhfU8RMfG7dPNVk9EWS3JbGmwow4R95Ae5P"

# Write the server config relative to the demo directory.
cat > "$SCRIPT_DIR/config/basis.toml" <<EOF
[server]
host = "127.0.0.1"
port = 3048
data_dir = "data"

[ergo]
basis_reserve_contract_p2s = "$RESERVE_P2S"
basis_token_reserve_contract_p2s = "$TOKEN_RESERVE_P2S"
tracker_nft_id = "$TRACKER_NFT_ID"
tracker_public_key = "$TRACKER_PUBKEY"
tracker_secret_key = "$TRACKER_SECRET"
reserve_token_id = "$USE_TOKEN_ID"
reserve_token_decimals = $USE_DECIMALS

[ergo.node]
start_height = $START_HEIGHT
node_url = "$NODE_URL"
api_key = "$NODE_API_KEY"

[transaction]
fee = 1000000
change_address = "$NODE_WALLET_ADDRESS"

[confirmation]
# For a real main-chain demo we still want a single confirmation block before
# treating notes as redeemable; depth 1 avoids waiting for a second block.
min_depth = 1
EOF

log_info "Checking tracker server at $SERVER_URL..."
if curl -s "$SERVER_URL/health" >/dev/null 2>&1; then
    log_error "A tracker server is already running at $SERVER_URL."
    log_error "This demo needs its own tracker (demo config + data dir). Stop it first."
    exit 1
fi

# Short tracker update interval so the tracker box is created soon after the first note.
export BASIS_TRACKER_UPDATE_INTERVAL_SECONDS="${BASIS_TRACKER_UPDATE_INTERVAL_SECONDS:-10}"

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

# The tracker box is created by the server after the first off-chain notes are
# issued (the AVL tree must be non-empty before the updater commits on-chain).
# The orchestrator waits for it before redemption.
log_info "Running Celaut + USE scenario..."

BASIS_MCP="$PROJECT_ROOT/target/release/basis-mcp" \
    BASIS_SERVER_URL="$SERVER_URL" \
    BASIS_NODE_URL="$NODE_URL" \
    BASIS_NODE_API_KEY="$NODE_API_KEY" \
    BASIS_TRACKER_SECRET="$TRACKER_SECRET" \
    DAVE_PUBKEY="$DAVE_PUBKEY" \
    DAVE_SECRET="$DAVE_SECRET" \
    USE_TOKEN_ID="$USE_TOKEN_ID" \
    DAVE_RESERVE_NFT_ID="$DAVE_RESERVE_NFT_ID" \
    TRACKER_NFT_ID="$TRACKER_NFT_ID" \
    python3 -u "$SCRIPT_DIR/orchestrator.py" --auto

log_info "Demo finished successfully."
