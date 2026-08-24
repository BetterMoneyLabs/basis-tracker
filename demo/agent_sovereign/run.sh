#!/usr/bin/env bash
# Basis Sovereign Agent Demo launcher
#
# A self-maintaining, self-improving agent economy: the sovereign agent buys its
# own compute, earns hash-verified bounties from an escrow-backed board,
# settles income on-chain in USE, upgrades its capabilities and survives a
# price shock — zero human actions.
#
# Required environment variables:
#   USE_TOKEN_ID             - hex token id of the USE stablecoin (64 chars)
#   TRACKER_NFT_ID           - NFT identifying the tracker instance
#   SOVEREIGN_RESERVE_NFT_ID - NFT for the sovereign's fixed seed reserve
#   BOUNTY_ESCROW_NFT_ID     - NFT for the bounty board's escrow reserve
#
# Optional:
#   BASIS_NODE_URL           - default http://127.0.0.1:9053
#   BASIS_NODE_API_KEY       - node API key (required for wallet operations)
#   BASIS_SERVER_URL         - default http://127.0.0.1:3048
#   BOUNTY_PUBKEY / BOUNTY_SECRET - bounty board keypair owning the escrow
#                               (defaults to the committed demo keypair; must
#                                match an existing escrow reserve on re-runs)
#   SEED_RESERVE_USE         - raw USE units for the sovereign seed (default 700)
#   ESCROW_RESERVE_USE       - raw USE units for the bounty escrow (default 1000)
#
# Usage:
#   ./demo/agent_sovereign/run.sh           # full demo
#   ./demo/agent_sovereign/run.sh --check   # preflight only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SERVER_URL="${BASIS_SERVER_URL:-http://127.0.0.1:3048}"
NODE_URL="${BASIS_NODE_URL:-http://127.0.0.1:9053}"
NODE_API_KEY="${BASIS_NODE_API_KEY:-}"

USE_TOKEN_ID="${USE_TOKEN_ID:-}"
TRACKER_NFT_ID="${TRACKER_NFT_ID:-}"
SOVEREIGN_RESERVE_NFT_ID="${SOVEREIGN_RESERVE_NFT_ID:-}"
BOUNTY_ESCROW_NFT_ID="${BOUNTY_ESCROW_NFT_ID:-}"

SEED_RESERVE_USE="${SEED_RESERVE_USE:-700}"      # raw units (3 decimals)
ESCROW_RESERVE_USE="${ESCROW_RESERVE_USE:-1000}" # raw units (3 decimals)

# USE has 3 decimals in this deployment.
USE_DECIMALS=3
USE_UNIT=$((10 ** USE_DECIMALS))

# Minimum wallet balance: tracker box + two reserve storage rents + fee boxes.
MIN_BALANCE_NANOERG=150000000   # 0.15 ERG

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
        TRACKER_NFT_ID="$TRACKER_NFT_ID" \
        SOVEREIGN_RESERVE_NFT_ID="$SOVEREIGN_RESERVE_NFT_ID" \
        BOUNTY_ESCROW_NFT_ID="$BOUNTY_ESCROW_NFT_ID" \
        python3 - <<'PY'
import json, os, sys, urllib.request, urllib.error

node_url = os.environ["NODE_URL"].rstrip("/")
api_key = os.environ.get("NODE_API_KEY", "")
prohibited_token_ids = {
    os.environ["TRACKER_NFT_ID"],
    os.environ["SOVEREIGN_RESERVE_NFT_ID"],
    os.environ["BOUNTY_ESCROW_NFT_ID"],
}

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

def box_tokens(box):
    return {a["tokenId"] for a in box.get("assets", [])}

def has_prohibited_token(box):
    return bool(box_tokens(box) & prohibited_token_ids)

fee = 1_000_000
fee_box_value = 50_000_000

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

# Fixed demo tracker keypair (same as demo/agent_celaut_use); override with
# TRACKER_PUBKEY/TRACKER_SECRET to reuse an existing on-chain tracker box.
DEFAULT_TRACKER_PUBKEY="039aa1478e19ad14e55c51bd306514636c608b0236edffbf03ca4028c063c4c99b"
DEFAULT_TRACKER_SECRET="bd9c331161cb8432c4037c198e33deb77c99b2b36a6f7956be1d1e6f829c5eca"

if [[ -n "${TRACKER_PUBKEY:-}" && -n "${TRACKER_SECRET:-}" ]]; then
    log_info "Using provided tracker keypair."
else
    TRACKER_PUBKEY="$DEFAULT_TRACKER_PUBKEY"
    TRACKER_SECRET="$DEFAULT_TRACKER_SECRET"
    log_info "Using fixed demo tracker keypair."
fi

check_env() {
    local missing=()
    for var in USE_TOKEN_ID TRACKER_NFT_ID SOVEREIGN_RESERVE_NFT_ID BOUNTY_ESCROW_NFT_ID; do
        if [[ -z "${!var:-}" ]]; then
            missing+=("$var")
        fi
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required environment variables: ${missing[*]}"
        echo "See demo/agent_sovereign/README.md"
        exit 1
    fi

    for var in USE_TOKEN_ID TRACKER_NFT_ID SOVEREIGN_RESERVE_NFT_ID BOUNTY_ESCROW_NFT_ID; do
        local val="${!var}"
        if [[ ${#val} -ne 64 ]] || ! [[ "$val" =~ ^[0-9a-fA-F]+$ ]]; then
            log_error "$var must be a 64-character hex string"
            exit 1
        fi
    done
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

    # Sovereign identity = first node wallet address.
    NODE_WALLET_ADDRESS=$(node_curl "/wallet/addresses" | python3 -c "import sys,json; print(json.load(sys.stdin)[0])")
    if [[ -z "$NODE_WALLET_ADDRESS" ]]; then
        log_error "Could not determine node wallet address"
        exit 1
    fi
    log_info "  Node wallet address (sovereign): $NODE_WALLET_ADDRESS"

    # Balances: ERG minimum, plus enough USE across wallet + already-locked
    # reserves to fund both seed and escrow.
    local balances
    balances=$(node_curl /wallet/balances)
    BALANCE_JSON="$balances" MIN_BALANCE_NANO="$MIN_BALANCE_NANOERG" \
        REQUIRED_USE_UNITS=$((SEED_RESERVE_USE + ESCROW_RESERVE_USE)) \
        USE_TOKEN_ID_CHECK="$USE_TOKEN_ID" \
        SEED_NFT_CHECK="$SOVEREIGN_RESERVE_NFT_ID" \
        ESCROW_NFT_CHECK="$BOUNTY_ESCROW_NFT_ID" \
        NODE_URL_CHECK="$NODE_URL" NODE_API_KEY_CHECK="$NODE_API_KEY" \
        python3 - <<'PY'
import json, os, sys, urllib.request

data = json.loads(os.environ['BALANCE_JSON'])
balance = int(data.get('balance', 0))
assets = data.get('confirmedTokens', data.get('assets', {})) or {}

def token_balance(token_id):
    if isinstance(assets, dict):
        return int(assets.get(token_id, 0))
    for entry in assets:
        if entry.get('tokenId') == token_id:
            return int(entry.get('amount', 0))
    return 0

min_balance_nano = int(os.environ['MIN_BALANCE_NANO'])
required_use_units = int(os.environ['REQUIRED_USE_UNITS'])
use_token_id = os.environ['USE_TOKEN_ID_CHECK']

ok = True
if balance < min_balance_nano:
    print(f'[ERROR] wallet balance {balance/1e9:.4f} ERG < required {min_balance_nano/1e9:.4f} ERG')
    ok = False
else:
    print(f'[INFO]   wallet balance {balance/1e9:.4f} ERG — sufficient')

def onchain_collateral(nft_id):
    """Sum USE held in unspent boxes containing this NFT."""
    url = f"{os.environ['NODE_URL_CHECK']}/blockchain/box/unspent/byTokenId/{nft_id}?limit=20"
    req = urllib.request.Request(url)
    api_key = os.environ.get('NODE_API_KEY_CHECK', '')
    if api_key:
        req.add_header('api_key', api_key)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            boxes = json.loads(resp.read().decode())
    except Exception:
        return 0
    total = 0
    for box in boxes:
        for asset in box.get('assets', []):
            if asset.get('tokenId') == use_token_id:
                total += int(asset.get('amount', 0))
    return total

locked = onchain_collateral(os.environ['SEED_NFT_CHECK']) + \
         onchain_collateral(os.environ['ESCROW_NFT_CHECK'])
available = token_balance(use_token_id) + locked
if available < required_use_units:
    print(f'[ERROR] USE available {available} (wallet + locked reserves) '
          f'< required {required_use_units}')
    ok = False
elif locked > 0:
    print(f'[INFO]   USE available {available} '
          f'({token_balance(use_token_id)} in wallet, {locked} locked in reserves) — sufficient')
else:
    print(f'[INFO]   USE token balance {available} — sufficient')

sys.exit(0 if ok else 1)
PY

    # All three NFTs must exist somewhere on-chain or in the wallet.
    for nft in "$SOVEREIGN_RESERVE_NFT_ID:sovereign-seed-reserve" \
               "$BOUNTY_ESCROW_NFT_ID:bounty-escrow-reserve" \
               "$TRACKER_NFT_ID:tracker"; do
        local id="${nft%%:*}"
        local label="${nft##*:}"
        local found
        found=$(node_curl "/blockchain/box/unspent/byTokenId/${id}?limit=1")
        if [[ "$found" == "[]" ]]; then
            log_error "NFT for $label ($id) not found on-chain or in wallet"
            exit 1
        fi
        log_info "  NFT for $label present on-chain"
    done
}

lock_reserves() {
    # Lock BOTH reserves before the server starts so the tracker's box updater
    # can never pick the unspent NFT boxes as fee inputs.
    log_info "Preflight: locking bounty-board escrow reserve (${ESCROW_RESERVE_USE} raw USE)..."
    if ! OWNER_PUBKEY="${BOUNTY_PUBKEY}" \
         RESERVE_NFT_ID="$BOUNTY_ESCROW_NFT_ID" \
         RESERVE_LABEL="bounty-escrow" \
         RESERVE_AMOUNT="$ESCROW_RESERVE_USE" \
         USE_TOKEN_ID="$USE_TOKEN_ID" TRACKER_NFT_ID="$TRACKER_NFT_ID" \
         BASIS_NODE_URL="$NODE_URL" BASIS_NODE_API_KEY="$NODE_API_KEY" \
         BASIS_TRACKER_SECRET="$TRACKER_SECRET" \
         python3 "$SCRIPT_DIR/reserve_helper.py"; then
        log_error "Bounty escrow reserve preflight failed"
        exit 1
    fi

    log_info "Preflight: locking sovereign seed reserve (${SEED_RESERVE_USE} raw USE)..."
    if ! OWNER_PUBKEY="$SOVEREIGN_PUBKEY" \
         RESERVE_NFT_ID="$SOVEREIGN_RESERVE_NFT_ID" \
         RESERVE_LABEL="sovereign-seed" \
         RESERVE_AMOUNT="$SEED_RESERVE_USE" \
         USE_TOKEN_ID="$USE_TOKEN_ID" TRACKER_NFT_ID="$TRACKER_NFT_ID" \
         BASIS_NODE_URL="$NODE_URL" BASIS_NODE_API_KEY="$NODE_API_KEY" \
         BASIS_TRACKER_SECRET="$TRACKER_SECRET" \
         python3 "$SCRIPT_DIR/reserve_helper.py"; then
        log_error "Sovereign seed reserve preflight failed"
        exit 1
    fi
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

# Bounty board demo keypair (must own the escrow reserve).
BOUNTY_PUBKEY="${BOUNTY_PUBKEY:-02ea220b8d7b6b1727b7e555493144f1b7085debdf24e41e547a687425b0d3c802}"
BOUNTY_SECRET="${BOUNTY_SECRET:-2acafd6320fbe5dbc5f1e84e8d075b0d1dadafffab1b9441a3ade1b508f83a64e}"

preflight

# Sovereign identity is the node wallet key (address resolved by preflight).
if [[ -n "${SOVEREIGN_PUBKEY_OVERRIDE:-}" ]]; then
    SOVEREIGN_PUBKEY="$SOVEREIGN_PUBKEY_OVERRIDE"
else
    SOVEREIGN_PUBKEY=$(echo "$NODE_WALLET_ADDRESS" | NODE_URL_S="$NODE_URL" NODE_API_KEY_S="$NODE_API_KEY" python3 -c "
import sys, json, os, urllib.request
address = sys.stdin.read().strip()
req = urllib.request.Request(os.environ['NODE_URL_S'] + '/utils/addressToRaw/' + address)
api_key = os.environ.get('NODE_API_KEY_S', '')
if api_key:
    req.add_header('api_key', api_key)
with urllib.request.urlopen(req, timeout=30) as resp:
    raw = json.loads(resp.read().decode())
if isinstance(raw, dict):
    pk = str(raw.get('raw') or raw.get('pubkey') or raw.get('publicKey') or raw.get('value'))
else:
    pk = str(raw)
print(pk)
")
fi
if [[ ${#SOVEREIGN_PUBKEY} -ne 66 ]]; then
    log_error "unexpected sovereign pubkey format/length: $SOVEREIGN_PUBKEY"
    exit 1
fi
log_info "Sovereign identity (node wallet pubkey): $SOVEREIGN_PUBKEY"

ensure_fee_box
lock_reserves

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

# Start scanning from a few blocks before the oldest reserve so the tracker
# picks both up. Fall back to current height - 5.
CURRENT_HEIGHT=$(node_curl /info | python3 -c "import sys, json; print(json.load(sys.stdin).get('fullHeight', 1))")
RESERVE_HEIGHT=$(node_curl "/blockchain/box/unspent/byTokenId/${SOVEREIGN_RESERVE_NFT_ID}?limit=1" | \
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
# Single confirmation depth keeps the mainnet demo moving.
min_depth = 1
EOF

log_info "Checking tracker server at $SERVER_URL..."
if curl -s "$SERVER_URL/health" >/dev/null 2>&1; then
    log_error "A tracker server is already running at $SERVER_URL."
    log_error "This demo needs its own tracker (demo config + data dir). Stop it first."
    exit 1
fi

# Short tracker update interval so notes commit soon after issuance.
export BASIS_TRACKER_UPDATE_INTERVAL_SECONDS="${BASIS_TRACKER_UPDATE_INTERVAL_SECONDS:-10}"

log_info "Starting tracker server..."
(
    cd "$SCRIPT_DIR"
    BASIS_SERVER_URL="$SERVER_URL" exec "$PROJECT_ROOT/target/release/basis_server"
) &
SERVER_PID=$!

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

log_info "Running sovereign agent scenario..."

BASIS_MCP="$PROJECT_ROOT/target/release/basis-mcp" \
    BASIS_SERVER_URL="$SERVER_URL" \
    BASIS_NODE_URL="$NODE_URL" \
    BASIS_NODE_API_KEY="$NODE_API_KEY" \
    USE_TOKEN_ID="$USE_TOKEN_ID" \
    TRACKER_NFT_ID="$TRACKER_NFT_ID" \
    BOUNTY_PUBKEY="$BOUNTY_PUBKEY" \
    BOUNTY_SECRET="$BOUNTY_SECRET" \
    python3 -u "$SCRIPT_DIR/orchestrator.py" --auto

log_info "Demo finished successfully."
