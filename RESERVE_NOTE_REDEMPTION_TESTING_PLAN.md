# Comprehensive Testing Plan: Reserve Creation, Note Signing, and Redemption Workflow

## Overview
This testing plan outlines a complete workflow for users to create reserves, sign notes against them, and redeem signed notes against the reserve using the Basis Tracker system. This plan includes usage of the `basis_cli` client which provides a command-line interface for interacting with the tracker server for account management, note operations, reserve status checks, and redemption processes.

## Prerequisites
- Access to Ergo node (mainnet/testnet)
- Wallet with ERG balance for creating reserves
- Basis Tracker server running
- `basis_cli` client installed and configured
- Two public-private key pairs (one for issuer, one for recipient)

## Client Setup with basis_cli
The `basis_cli` client provides a comprehensive command-line interface for interacting with the Basis Tracker system. It handles account management, cryptographic operations, and API communication.

### Client Installation
```bash
# Build the cli client from the workspace
cargo build -p basis_cli

# Or run directly
cargo run -p basis_cli -- --help
```

### Account Management
The client manages accounts with secp256k1 key pairs that can serve as either issuers or recipients in note operations:
```bash
# Create a new account for issuing notes
basis-cli account create issuer_account

# Create a new account for receiving notes
basis-cli account create recipient_account

# List all accounts
basis-cli account list

# Switch between accounts
basis-cli account switch issuer_account
```

### Client Configuration
The client stores configuration in `~/.basis/cli.toml` and supports custom server URLs:
```bash
# Specify custom server URL
basis-cli --server-url http://localhost:3048 status
```

## Phase 1: Reserve Creation

### Step 1: Generate NFT for Reserve
1. Create an NFT to associate with your reserve box
2. Record the NFT ID (token ID) - this will be used in the reserve creation

### Step 2: Create Reserve Using Client (Recommended)
1. Use the client to create a reserve (using the currently selected account as owner):
```bash
basis-cli reserve create --nft-id {tracker_nft_id} --amount {erg_amount}
```

2. Or specify a specific owner public key:
```bash
basis-cli reserve create --nft-id {tracker_nft_id} --owner {owner_pubkey} --amount {erg_amount}
```

3. The client will return a payload that can be submitted to an Ergo node's `/wallet/payment/send` API

### Step 3: Execute Reserve Transaction with Ergo Node
1. Take the output from Step 2 and submit it to your Ergo node's `/wallet/payment/send` API:
```bash
curl -X POST "http://your-ergo-node:9053/wallet/payment/send" \
  -H "Content-Type: application/json" \
  -H "api_key: your_api_key" \
  -d '{payload_from_step_2}'
```

2. Verify that the transaction is successful and note the reserve box ID for testing

### Step 4: Verify Reserve Creation with Client
1. Use the client to check key status: `basis-cli reserve status {owner_pubkey}`
2. Verify the reserve appears with the correct owner and amount
3. Check collateralization status and debt information

## Phase 2: Note Creation and Signing

### Step 1: Prepare Note Data
1. Create note parameters:
   - `recipient_pubkey`: Recipient's 33-byte compressed public key (can be obtained with `basis-cli account info` when recipient account is selected)
   - `amount`: Amount of debt obligation (in nanoERG)
   - `timestamp`: Current Unix timestamp

### Step 2: Sign the Note
1. The `basis_cli` client handles note signing internally using the issuer's private key from the selected account
2. The client creates the signing message by concatenating:
   - Recipient public key (33 bytes)
   - Amount as big-endian 8-byte value
   - Timestamp as big-endian 8-byte value
3. Uses Schnorr signature algorithm with the issuer's private key from the current account

### Step 3: Submit Note to Tracker Using Client
1. Use the client to create a note (requires the issuer account to be selected):
```bash
# Note: The client would handle the signing automatically using the current account
basis-cli note create --recipient {recipient_pubkey} --amount {amount} --timestamp {timestamp}
```
2. The client automatically includes the signature when submitting the note to the tracker server

### Step 4: Verify Note Creation with Client
1. Verify the note appears in tracker: `basis-cli note list-issuer {issuer_pubkey}`
2. Check that key status reflects the new debt: `basis-cli reserve status {issuer_pubkey}`
3. Confirm collateralization ratio is adequate

## Phase 3: Redemption Process

### Step 1: Initiate Redemption Using Client
1. Use the client to initiate redemption (requires recipient account to be selected):
```bash
basis-cli note redeem --issuer {issuer_pubkey} --recipient {recipient_pubkey} --amount {amount} --timestamp {timestamp}
```
2. The client will return redemption details including a redemption ID

### Step 2: Verify Redemption Preparation with Client
1. Check that a redemption ID is returned by the client
2. Verify redemption status using: `basis-cli note get-proof --issuer {issuer_pubkey} --recipient {recipient_pubkey}`
3. Confirm proof is available if needed for blockchain verification

### Step 3: Complete Redemption Using Client
1. Use the client to complete redemption:
```bash
basis-cli note complete-redemption --issuer {issuer_pubkey} --recipient {recipient_pubkey} --amount {redeemed_amount}
```

### Step 4: Verify Redemption Results with Client
1. Check that the note's `amount_redeemed` has increased using `basis-cli note get --issuer {issuer_pubkey} --recipient {recipient_pubkey}`
2. Verify that the issuer's debt is reduced: `basis-cli reserve status {issuer_pubkey}`
3. Ensure the collateralization ratio is updated

## Phase 4: End-to-End Validation Tests

### Test 1: Full Reserve-Note-Redemption Flow
1. Create a reserve with sufficient collateral
2. Create multiple signed notes against the reserve
3. Verify all notes appear in the tracker
4. Redeem some of the notes
5. Verify the redemption changes are reflected

### Test 2: Collateralization Validation
1. Create a reserve with minimum required collateral
2. Sign notes up to the collateralization limit
3. Attempt to create a note that exceeds the limit
4. Verify that the system rejects the over-collateralized note

### Test 3: Invalid Signature Handling
1. Create a note with an invalid signature
2. Submit to the tracker
3. Verify the system rejects the note with invalid signature error

### Test 4: Reserve Top-Up Scenario
1. Create a reserve with limited collateral
2. Submit notes that approach the limit
3. Top up the reserve with additional collateral
4. Submit additional notes and verify they are accepted

## Phase 5: Automated Testing Scripts

### Script 1: Basic Workflow Test with Client
**Using HTTP API directly:**
```bash
# 1. Create reserve and get payload
curl -X POST "$TRACKER_SERVER/reserves/create" \
  -H "Content-Type: application/json" \
  -d '{"nft_id": "$NFT_ID", "owner_pubkey": "$OWNER_PUBKEY", "erg_amount": $AMOUNT}'

# 2. Execute transaction with Ergo node
# (Use response from step 1 as payload to /wallet/payment/send)

# 3. Create and sign a note
# (Sign with issuer private key)

# 4. Submit note to tracker
curl -X POST "$TRACKER_SERVER/notes" \
  -H "Content-Type: application/json" \
  -d '{"recipient_pubkey": "...", "amount": "...", "timestamp": "...", "signature": "...", "issuer_pubkey": "..."}'

# 5. Initiate and complete redemption
curl -X POST "$TRACKER_SERVER/redeem" \
  -H "Content-Type: application/json" \
  -d '{"issuer_pubkey": "...", "recipient_pubkey": "...", "amount": "...", "timestamp": "..."}'
```

**Using basis_cli client (Recommended approach):**
```bash
# 1. Set up accounts
basis-cli account create issuer_account
basis-cli account create recipient_account

# 2. Switch to issuer account and create reserve
basis-cli account switch issuer_account
# Get the public key for the issuer account
ISSUER_PUBKEY=$(basis-cli account info | grep "Public Key" | cut -d':' -f2 | tr -d ' ')
# Create the reserve (this creates the payload for Ergo node)
basis-cli reserve create --nft-id {tracker_nft_id} --amount 1000000000
# Execute the reserve creation transaction with your Ergo node
# (See step 3 for how to submit the payload to Ergo node)

# 3. Create and submit note (issuer account must be selected)
basis-cli note create --recipient {recipient_pubkey} --amount {amount} --timestamp {timestamp}

# 4. Switch to recipient account and initiate redemption
basis-cli account switch recipient_account
basis-cli note redeem --issuer {issuer_pubkey} --recipient {recipient_pubkey} --amount {amount} --timestamp {timestamp}

# 5. Complete redemption
basis-cli note complete-redemption --issuer {issuer_pubkey} --recipient {recipient_pubkey} --amount {redeemed_amount}
```

**Complete end-to-end example using client only (reserving actual Ergo transaction):**
```bash
# 1. Set up accounts
basis-cli account create issuer_account
basis-cli account create recipient_account

# 2. Get account public keys
basis-cli account switch issuer_account
ISSUER_PUBKEY=$(basis-cli account info | grep -o "0[1-9a-fA-F]\{65\}")
echo "Issuer pubkey: $ISSUER_PUBKEY"

basis-cli account switch recipient_account
RECIPIENT_PUBKEY=$(basis-cli account info | grep -o "0[1-9a-fA-F]\{65\}")
echo "Recipient pubkey: $RECIPIENT_PUBKEY"

# 3. Create reserve payload (without executing on chain - for testing purposes)
# Note: In real usage, you'd submit the payload from this command to your Ergo node
RESERVE_PAYLOAD=$(basis-cli reserve create --nft-id {tracker_nft_id} --owner $ISSUER_PUBKEY --amount 1000000000)

# 4. Verify reserve status before creating notes
basis-cli reserve status --issuer $ISSUER_PUBKEY

# 5. Create and submit note
basis-cli account switch issuer_account
basis-cli note create --recipient $RECIPIENT_PUBKEY --amount 500000000 --timestamp $(date +%s)

# 6. Verify note was created
basis-cli note list-issuer $ISSUER_PUBKEY
basis-cli note list-recipient $RECIPIENT_PUBKEY

# 7. Verify updated reserve status (should show new debt)
basis-cli reserve status --issuer $ISSUER_PUBKEY

# 8. Initiate and complete redemption
basis-cli account switch recipient_account
basis-cli note redeem --issuer $ISSUER_PUBKEY --recipient $RECIPIENT_PUBKEY --amount 250000000 --timestamp $(date +%s)
```

### Script 2: Client-Based Validation Tests
**Account Management Tests:**
```bash
# Create multiple accounts for testing
basis-cli account create test_issuer
basis-cli account create test_recipient
basis-cli account list

# Verify account information
basis-cli account info
```

**Reserve Management Tests:**
```bash
# Create a reserve using the current account as owner
basis-cli reserve create --nft-id {tracker_nft_id} --amount 1000000000

# Create a reserve specifying a specific owner public key
basis-cli reserve create --nft-id {tracker_nft_id} --owner {owner_pubkey} --amount 1000000000

# Check reserve status and collateralization
basis-cli reserve status --issuer {pubkey}

# Check collateralization ratio details
basis-cli reserve collateralization --issuer {pubkey}
```

**Note Management Tests:**
```bash
# List notes for issuer and recipient
basis-cli note list-issuer {issuer_pubkey}
basis-cli note list-recipient {recipient_pubkey}

# Get specific note details
basis-cli note get --issuer {issuer_pubkey} --recipient {recipient_pubkey}
```

**Status and Proof Verification:**
```bash
# Check reserve status and collateralization
basis-cli reserve status {pubkey}

# Get redemption proofs
basis-cli note get-proof --issuer {issuer_pubkey} --recipient {recipient_pubkey}

# Check system events
basis-cli status
```

### Script 3: Interactive Mode Testing
The client provides an interactive mode for testing workflows:
```bash
basis-cli interactive
# Then run commands interactively:
# - account create <name>
# - note create --recipient <pubkey> --amount <amount>
# - reserve status <pubkey>
# - etc.
```

### Script 4: Stress Testing with Client
- Create multiple accounts in sequence using client
- Submit notes through client commands in parallel processes
- Execute redemptions via client commands concurrently
- Verify data consistency using client status commands

### Script 5: Edge Case Testing with Client
- Notes with invalid amounts using client validation
- Invalid public key formats tested via client
- Client validation of timestamp boundaries
- Test client behavior with incorrect account selection

## Phase 6: Monitoring and Validation Points

### Tracker Server Logs
- Monitor for successful note creation events
- Track redemption initiation and completion
- Watch for any validation failures

### Client-Based Monitoring
- Use `basis-cli status` to monitor server health
- Use `basis-cli account list` to verify account states
- Use `basis-cli note list-issuer` and `basis-cli note list-recipient` to track note states
- Use `basis-cli reserve status {pubkey}` to monitor collateralization ratios
- Use `basis-cli reserve create --nft-id <id> --amount <amount>` to generate reserve creation payloads
- Use `basis-cli note get-proof` to retrieve redemption proofs

### Blockchain Verification
- Verify reserve box creation on-chain
- Confirm register values (R4, R5) match expected values
- Validate that ERG amount matches the request

### Balance Verification
- Verify issuer debt amounts in the tracker using client commands
- Check recipient note receipts via client
- Confirm redemption amounts are properly subtracted using client verification
- Use client to validate that debt and collateral amounts are accurate

## Success Criteria
- All created notes are properly stored in the tracker and accessible via client
- Collateralization ratios are accurately calculated and reflected in client status
- Redemption processes reduce outstanding debt appropriately and are verifiable via client
- No invalid notes are accepted by the system during client-based submissions
- All blockchain interactions complete successfully
- Tracker state remains consistent with blockchain state after client operations
- Client commands return expected results and error messages for edge cases

This testing plan ensures a comprehensive validation of the complete workflow from reserve creation through note redemption while covering various edge cases and stress scenarios, with special attention to client-based workflows using the `basis_cli` interface.