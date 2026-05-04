#!/bin/bash
#
# Basis Simple Demo
# 
# Demonstrates the Basis protocol flow:
# 1. Alice issues IOU note to Bob with dual signatures
# 2. Note is saved and can be redeemed
#
# This demo creates an IOU note with test keys and saves it to a file.
# In a real scenario, the note would be redeemed against an on-chain reserve.
#

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_DIR="$DEMO_DIR/output"
CLI="cargo run -p basis_cli --quiet --"

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "========================================"
echo "  Basis Simple Demo"
echo "========================================"
echo ""
echo "This demo shows:"
echo "  1. Alice issues IOU note to Bob"
echo "  2. Both Alice and Tracker sign the note"
echo "  3. Note is saved for later redemption"
echo ""
echo "Prerequisites:"
echo "  - Rust toolchain (cargo)"
echo "  - No running server required for note creation"
echo ""

# Step 1: Create IOU Note
echo "--- Step 1: Creating IOU Note ---"
echo "Alice issues 0.05 ERG IOU to Bob with Tracker signature"
echo ""

$CLI note create \
  --demo \
  --amount 50000000 \
  --output "$OUTPUT_DIR/note.json"

echo ""
echo "========================================"
echo "  Demo Complete!"
echo "========================================"
echo ""
echo "Generated files:"
echo "  - $OUTPUT_DIR/note.json (IOU note with dual signatures)"
echo ""
echo "Note contents:"
cat "$OUTPUT_DIR/note.json" | head -20
echo "  ..."
echo ""
echo "The note includes:"
echo "  ✓ Alice's signature (payer/reserve owner)"
echo "  ✓ Tracker's signature (off-chain witness)"
echo "  ✓ 48-byte signing message (key || totalDebt || timestamp)"
echo ""
echo "Next steps (requires Ergo node and reserve):"
echo "  1. Sign redemption transaction via Ergo node"
echo "  2. Broadcast to blockchain"
echo ""
echo "For full documentation, see: demo/README.md"
