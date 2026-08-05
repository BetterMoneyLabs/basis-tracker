#!/usr/bin/env bash
# Basis LETS Tutorial Launcher
# Sets up a local mutual-credit network where each member uses the basis-ui TUI wallet.
# No reserves, collateral, or redemption; pure trust-based IOU notes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SERVER_URL="${BASIS_SERVER_URL:-http://127.0.0.1:3048}"
MEMBERS=("alice" "bob" "carol")
CREDIT_LIMIT=5000000000  # 5 ERG in nanoERG
CLEAN=false
TMUX=false
RELEASE=false

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $1"; }

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  --members alice,bob,carol   Comma-separated list of LETS members (default: ${MEMBERS[*]})
  --credit-limit N            Maximum cumulative debt per member in nanoERG (default: $CREDIT_LIMIT)
  --tmux                      Launch member TUI wallets in separate tmux windows
  --clean                     Remove previous demo state before starting
  --release                   Build release binaries instead of debug
  --help                      Show this help message

Examples:
  $0                          # Set up alice, bob, carol; print launch commands
  $0 --tmux                   # Set up members and launch them in tmux
  $0 --members alice,bob      # Two-member LETS
  $0 --clean --tmux           # Fresh start with tmux launch
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --members)
            IFS=',' read -ra MEMBERS <<< "$2"
            shift 2
            ;;
        --credit-limit)
            CREDIT_LIMIT="$2"
            shift 2
            ;;
        --tmux)
            TMUX=true
            shift
            ;;
        --clean)
            CLEAN=true
            shift
            ;;
        --release)
            RELEASE=true
            shift
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

if [[ ${#MEMBERS[@]} -lt 2 ]]; then
    log_error "At least 2 members are required for a LETS."
    exit 1
fi

if [[ "$RELEASE" == true ]]; then
    PROFILE="release"
else
    PROFILE="debug"
fi

BASIS_SERVER="$PROJECT_ROOT/target/$PROFILE/basis_server"
BASIS_CLI="$PROJECT_ROOT/target/$PROFILE/basis_cli"
BASIS_UI="$PROJECT_ROOT/target/$PROFILE/basis-ui"

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        log_info "Stopping tracker server (pid $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

log_info "Building basis_server, basis_cli, and basis-ui ($PROFILE)..."
cargo build -p basis_server -p basis_cli -p basis_app

if [[ "$CLEAN" == true ]]; then
    log_warn "Cleaning previous LETS demo state..."
    rm -rf "$SCRIPT_DIR/data" "$SCRIPT_DIR/config"
fi

log_step "Generating tracker configuration..."
mkdir -p "$SCRIPT_DIR/config"

# Generate a fresh tracker keypair for the demo.
KEYPAIR_JSON=$("$BASIS_CLI" generate-keypair --json)
TRACKER_PUBKEY=$(echo "$KEYPAIR_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['public_key_hex'])")

# Random 32-byte tracker NFT ID (hex). It does not need to exist on-chain for
# the pure-credit demo, but the server requires a validly formatted value.
TRACKER_NFT_ID=$(python3 -c "import secrets; print(secrets.token_hex(32))")

# Hardcoded reserve contract P2S from the server defaults; only used to satisfy
# config validation in this no-reserve demo.
RESERVE_P2S="3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT"

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
if curl -sf "$SERVER_URL/" >/dev/null 2>&1; then
    log_warn "A tracker server is already running at $SERVER_URL; reusing it."
else
    log_info "Starting tracker server..."
    (
        cd "$SCRIPT_DIR"
        BASIS_SERVER_URL="$SERVER_URL" exec "$BASIS_SERVER"
    ) &
    SERVER_PID=$!

    for i in {1..30}; do
        if curl -sf "$SERVER_URL/" >/dev/null 2>&1; then
            log_info "Tracker server is ready."
            break
        fi
        sleep 0.5
    done

    if ! curl -sf "$SERVER_URL/" >/dev/null 2>&1; then
        log_error "Tracker server failed to start."
        exit 1
    fi
fi

log_step "Creating LETS member accounts..."
declare -A MEMBER_PUBKEYS
declare -A MEMBER_HOMES

for member in "${MEMBERS[@]}"; do
    member_home="$SCRIPT_DIR/data/$member"
    MEMBER_HOMES[$member]="$member_home"
    mkdir -p "$member_home/.basis"

    if [[ -f "$member_home/.basis/cli.toml" ]]; then
        log_warn "Account for '$member' already exists; skipping creation."
    else
        HOME="$member_home" "$BASIS_CLI" account create "$member" >/dev/null
    fi

    pubkey=$(HOME="$member_home" "$BASIS_CLI" account info --json | python3 -c "import sys, json; print(json.load(sys.stdin)['pubkey_hex'])")
    MEMBER_PUBKEYS[$member]="$pubkey"
    log_info "  $member -> ${pubkey:0:16}...${pubkey: -10}"
done

log_step "Writing TUI configs and acceptance policies..."
for member in "${MEMBERS[@]}"; do
    member_home="${MEMBER_HOMES[$member]}"
    policy_file="$member_home/.basis/policy.toml"
    ui_toml="$member_home/.basis/ui.toml"

    # Build whitelist of all *other* members.
    holders=()
    for other in "${MEMBERS[@]}"; do
        if [[ "$other" != "$member" ]]; then
            holders+=("\"${MEMBER_PUBKEYS[$other]}\"")
        fi
    done
    holders_list=$(IFS=','; echo "${holders[*]}")

    cat > "$policy_file" <<EOF
default = "reject"
root = "lets_trust"

[[predicates]]
name = "lets_members"
type = "whitelist"
holders = [$holders_list]
max_debt = $CREDIT_LIMIT

[[predicates]]
name = "lets_trust"
type = "any_of"
predicates = ["lets_members"]
EOF

    # Build address book of all other members.
    address_book_entries=()
    for other in "${MEMBERS[@]}"; do
        if [[ "$other" != "$member" ]]; then
            address_book_entries+=("$other = \"${MEMBER_PUBKEYS[$other]}\"")
        fi
    done
    address_book_toml=$(IFS=$'\n'; echo "${address_book_entries[*]}")

    cat > "$ui_toml" <<EOF
server_url = "$SERVER_URL"
current_account = "$member"

[acceptance]
default = "reject"
root = "lets_trust"

[[acceptance.predicates]]
name = "lets_members"
type = "whitelist"
holders = [$holders_list]
max_debt = $CREDIT_LIMIT

[[acceptance.predicates]]
name = "lets_trust"
type = "any_of"
predicates = ["lets_members"]

[address_book]
$address_book_toml
EOF

    log_info "  $member: policy written and address book seeded."
done

log_step "Uploading acceptance policies to the tracker..."
for member in "${MEMBERS[@]}"; do
    member_home="${MEMBER_HOMES[$member]}"
    policy_file="$member_home/.basis/policy.toml"

    HOME="$member_home" "$BASIS_CLI" acceptance upload --policy-file "$policy_file" >/dev/null
    log_info "  $member policy uploaded."
done

echo ""
log_info "LETS setup complete."
echo ""

if [[ "$TMUX" == true ]]; then
    session="basis-lets-$(date +%s)"
    log_info "Launching tmux session '$session'..."
    tmux new-session -d -s "$session" -n "tracker" "cd '$SCRIPT_DIR' && echo 'Tracker server is running in the background.' && read -p 'Press Enter to close...'"

    first=true
    for member in "${MEMBERS[@]}"; do
        member_home="${MEMBER_HOMES[$member]}"
        cmd="cd '$PROJECT_ROOT' && HOME='$member_home' '$BASIS_UI'"
        if [[ "$first" == true ]]; then
            tmux rename-window -t "$session:0" "$member"
            tmux send-keys -t "$session:0" "$cmd" C-m
            first=false
        else
            tmux new-window -t "$session" -n "$member" "$cmd"
        fi
    done

    tmux select-window -t "$session:0"
    echo ""
    echo -e "${GREEN}Attach with:${NC} tmux attach -t $session"
    echo -e "${YELLOW}Note:${NC} Press Ctrl+B then D to detach; run the script again with --clean to reset."
else
    echo -e "${GREEN}Open one terminal per member and run:${NC}"
    for member in "${MEMBERS[@]}"; do
        member_home="${MEMBER_HOMES[$member]}"
        echo "  $member: HOME='$member_home' '$BASIS_UI'"
    done
fi

echo ""
echo -e "${BLUE}Suggested scenario:${NC}"
echo "  1. Alice pays Bob    2 ERG for bread"
echo "  2. Bob pays Carol    1 ERG for a ride"
echo "  3. Carol pays Alice  1.5 ERG for tutoring"
echo ""
echo -e "${BLUE}Expected final net positions:${NC}"
echo "  Alice:  -0.5 ERG"
echo "  Bob:    +1.0 ERG"
echo "  Carol:  -0.5 ERG"
echo ""
echo "Use the TUI's 'Create Note' screen to issue notes, and watch the"
echo "stats screen update assets, liabilities, and net position."

# Keep the script alive when not in tmux so the tracker stays running.
if [[ "$TMUX" == false ]]; then
    echo ""
    read -p "Press Enter to stop the tracker server and exit..."
fi
