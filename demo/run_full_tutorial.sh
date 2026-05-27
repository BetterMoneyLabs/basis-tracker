#!/bin/bash
# Basis Protocol - Full Tutorial Automation Script
# Alice → Bob Payment & Redemption with Real Tracker

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
ALICE_PUBKEY="0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83"
BOB_PUBKEY="03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea"
TRACKER_PUBKEY="030303030303030303030303030303030303030303030303030303030303030303"
DEFAULT_COLLATERAL=100000000  # 0.1 ERG in nanoERG
DEFAULT_NOTE_AMOUNT=50000000  # 0.05 ERG in nanoERG
DEFAULT_REDEEM_AMOUNT=25000000 # 0.025 ERG in nanoERG
SERVER_URL="http://localhost:3048"
NODE_URL="http://localhost:9053"

# Parse arguments
STEP="all"
while [[ $# -gt 0 ]]; do
    case $1 in
        --step)
            STEP="$2"
            shift 2
            ;;
        --collateral)
            DEFAULT_COLLATERAL="$2"
            shift 2
            ;;
        --note-amount)
            DEFAULT_NOTE_AMOUNT="$2"
            shift 2
            ;;
        --redeem-amount)
            DEFAULT_REDEEM_AMOUNT="$2"
            shift 2
            ;;
        --server-url)
            SERVER_URL="$2"
            shift 2
            ;;
        --node-url)
            NODE_URL="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --step reserve|note|redeem|all  Run specific step (default: all)"
            echo "  --collateral AMOUNT             Reserve collateral in nanoERG (default: 100000000)"
            echo "  --note-amount AMOUNT           Note amount in nanoERG (default: 50000000)"
            echo "  --redeem-amount AMOUNT         Redemption amount in nanoERG (default: 25000000)"
            echo "  --server-url URL               Tracker server URL (default: http://localhost:3048)"
            echo "  --node-url URL                 Ergo node URL (default: http://159.89.116.15:11088)"
            echo "  --help                         Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                              # Run full tutorial"
            echo "  $0 --step reserve               # Deploy reserve only"
            echo "  $0 --step note                  # Create IOU note only"
            echo "  $0 --step redeem                # Generate redemption tx only"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check basis_cli exists
    if [ -f "./target/debug/basis_cli" ]; then
        BASIS_CLI="./target/debug/basis_cli"
    elif [ -f "./target/release/basis_cli" ]; then
        BASIS_CLI="./target/release/basis_cli"
    else
        log_error "basis_cli not found"
        log_info "Please build the CLI first: cargo build -p basis_cli"
        exit 1
    fi
    log_info "Using CLI: $BASIS_CLI"
    
    # Check tracker server
    if ! curl -s "$SERVER_URL/health" > /dev/null 2>&1; then
        log_warn "Tracker server not reachable at $SERVER_URL"
        log_info "Please start the tracker server first: cargo run -p basis_server"
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    else
        log_success "Tracker server is reachable"
    fi
    
    # Check Ergo node
    if ! curl -s "$NODE_URL/info" > /dev/null 2>&1; then
        log_warn "Ergo node not reachable at $NODE_URL"
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    else
        log_success "Ergo node is reachable"
    fi
    
    log_success "Prerequisites check complete"
}

# Step 1: Deploy Reserve
step_reserve() {
    log_info "=== Step 1: Deploying Reserve ==="
    log_info "Alice (Issuer) will create a reserve with $DEFAULT_COLLATERAL nanoERG collateral"
    
    # Get tracker NFT ID from server config
    log_info "Fetching tracker NFT ID from server..."
    NFT_ID=$(curl -s "$SERVER_URL/config" | grep -o '"tracker_nft_id":"[^"]*"' | cut -d'"' -f4 || echo "")
    
    if [ -z "$NFT_ID" ]; then
        log_warn "Could not fetch tracker NFT ID from server"
        read -p "Enter tracker NFT ID (or press Enter to use placeholder): " NFT_ID
        if [ -z "$NFT_ID" ]; then
            NFT_ID="69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
            log_warn "Using placeholder NFT ID: $NFT_ID"
        fi
    fi
    
    log_info "Creating reserve with NFT ID: $NFT_ID"
    
    # Generate reserve creation payload
    $BASIS_CLI reserve create \
        --owner "$ALICE_PUBKEY" \
        --amount "$DEFAULT_COLLATERAL" \
        --nft-id "$NFT_ID" \
        > reserve_payload.json 2>&1
    
    if [ $? -eq 0 ]; then
        log_success "Reserve creation payload generated: reserve_payload.json"
        log_info "Next steps:"
        log_info "  1. Review reserve_payload.json"
        log_info "  2. Submit to Ergo wallet:"
        log_info "     curl -X POST $NODE_URL/wallet/transaction/send \\"
        log_info "       -H 'Content-Type: application/json' \\"
        log_info "       -H 'api_key: YOUR_API_KEY' \\"
        log_info "       -d @reserve_payload.json"
        log_info "  3. Wait for confirmation"
        log_info "  4. Verify: $BASIS_CLI reserve status --issuer $ALICE_PUBKEY"
    else
        log_error "Failed to generate reserve payload"
        cat reserve_payload.json
        return 1
    fi
}

# Step 2: Create IOU Note
step_note() {
    log_info "=== Step 2: Creating IOU Note (Alice → Bob) ==="
    log_info "Amount: $DEFAULT_NOTE_AMOUNT nanoERG"
    
    # Create note using demo mode (keys from participants.csv)
    $BASIS_CLI note create \
        --demo \
        --amount "$DEFAULT_NOTE_AMOUNT" \
        --output alice_to_bob_note.json
    
    if [ $? -eq 0 ]; then
        log_success "IOU note created: alice_to_bob_note.json"
        log_info "Note details:"
        cat alice_to_bob_note.json | jq '{payerKey, payeeKey, totalDebt, totalDebtERG, timestamp}'
        
        log_info "Next steps:"
        log_info "  1. Note has been sent to tracker server"
        log_info "  2. Verify: $BASIS_CLI note get --issuer $ALICE_PUBKEY --recipient $BOB_PUBKEY"
    else
        log_error "Failed to create IOU note"
        return 1
    fi
}

# Step 3: Generate Redemption Transaction
step_redeem() {
    log_info "=== Step 3: Generating Redemption Transaction ==="
    log_info "Bob will redeem $DEFAULT_REDEEM_AMOUNT nanoERG from Alice's reserve"
    
    # Check if note exists
    log_info "Checking note status..."
    $BASIS_CLI note get \
        --issuer "$ALICE_PUBKEY" \
        --recipient "$BOB_PUBKEY" > /dev/null 2>&1
    
    if [ $? -ne 0 ]; then
        log_warn "Note not found. Creating note first..."
        step_note
    fi
    
    # Generate redemption transaction
    $BASIS_CLI transaction generate-redemption \
        --issuer-pubkey "$ALICE_PUBKEY" \
        --recipient-pubkey "$BOB_PUBKEY" \
        --amount "$DEFAULT_REDEEM_AMOUNT" \
        --output-file redemption_tx.json
    
    if [ $? -eq 0 ]; then
        log_success "Redemption transaction generated: redemption_tx.json"
        log_info "Transaction details:"
        cat redemption_tx.json | jq '{requests: [.requests[] | {address: .address[:20] + "...", value}], fee}'
        
        log_info "Next steps:"
        log_info "  1. Review redemption_tx.json"
        log_info "  2. Sign with Ergo wallet:"
        log_info "     curl -X POST $NODE_URL/wallet/transaction/sign \\"
        log_info "       -H 'Content-Type: application/json' \\"
        log_info "       -H 'api_key: BOB_API_KEY' \\"
        log_info "       -d @redemption_tx.json"
        log_info "  3. Broadcast signed transaction"
        log_info "  4. Verify: $BASIS_CLI reserve status --issuer $ALICE_PUBKEY"
    else
        log_error "Failed to generate redemption transaction"
        return 1
    fi
}

# Main execution
main() {
    echo "=========================================="
    echo "  Basis Protocol - Full Tutorial"
    echo "  Alice → Bob Payment & Redemption"
    echo "=========================================="
    echo ""
    
    check_prerequisites
    
    case "$STEP" in
        reserve)
            step_reserve
            ;;
        note)
            step_note
            ;;
        redeem)
            step_redeem
            ;;
        all)
            step_reserve
            echo ""
            read -p "Press Enter to continue to Step 2 (Create IOU Note)..."
            echo ""
            
            step_note
            echo ""
            read -p "Press Enter to continue to Step 3 (Generate Redemption)..."
            echo ""
            
            step_redeem
            echo ""
            log_success "=== Tutorial Complete ==="
            log_info "Generated files:"
            log_info "  - reserve_payload.json"
            log_info "  - alice_to_bob_note.json"
            log_info "  - redemption_tx.json"
            ;;
        *)
            log_error "Unknown step: $STEP"
            log_info "Use --help for usage information"
            exit 1
            ;;
    esac
}

main
